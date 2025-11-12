use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use bc_components::ARID;
use bc_envelope::Envelope;
use bc_ur::prelude::*;
use tokio::time::sleep;

use crate::{Error, KvStore, Result};

/// In-memory key-value store for Gordian Envelopes.
///
/// Provides volatile storage with TTL support and automatic cleanup of
/// expired entries.
#[derive(Clone)]
pub struct MemoryKv {
    storage: Arc<RwLock<HashMap<ARID, StorageEntry>>>,
}

#[derive(Clone)]
struct StorageEntry {
    envelope_cbor: Vec<u8>,
    expires_at: Option<Instant>,
}

impl MemoryKv {
    /// Create a new in-memory key-value store.
    pub fn new() -> Self {
        Self { storage: Arc::new(RwLock::new(HashMap::new())) }
    }

    /// Check if an ARID exists and is not expired.
    fn check_exists(&self, arid: &ARID) -> Result<bool> {
        let storage = self.storage.read().unwrap();

        if let Some(entry) = storage.get(arid) {
            if let Some(expires_at) = entry.expires_at
                && Instant::now() >= expires_at
            {
                drop(storage);
                // Entry is expired, remove it
                let mut storage = self.storage.write().unwrap();
                storage.remove(arid);
                return Ok(false);
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl Default for MemoryKv {
    fn default() -> Self { Self::new() }
}

#[async_trait::async_trait(?Send)]
impl KvStore for MemoryKv {
    async fn put(
        &self,
        arid: &ARID,
        envelope: &Envelope,
        ttl_seconds: Option<u64>,
        verbose: bool,
    ) -> Result<String> {
        use crate::logging::verbose_println;

        let mut storage = self.storage.write().unwrap();

        // Check if already exists
        if storage.contains_key(arid) {
            if verbose {
                verbose_println(&format!(
                    "PUT {} ALREADY_EXISTS",
                    arid.ur_string()
                ));
            }
            return Err(Error::AlreadyExists { arid: arid.ur_string() });
        }

        let expires_at =
            ttl_seconds.map(|ttl| Instant::now() + Duration::from_secs(ttl));
        let envelope_cbor = envelope.to_cbor_data();

        storage.insert(*arid, StorageEntry { envelope_cbor, expires_at });

        if verbose {
            let ttl_msg = ttl_seconds
                .map(|ttl| format!(" (TTL {}s)", ttl))
                .unwrap_or_default();
            verbose_println(&format!(
                "PUT {}{} OK (Memory)",
                arid.ur_string(),
                ttl_msg
            ));
        }

        Ok("Stored in memory".to_string())
    }

    async fn get(
        &self,
        arid: &ARID,
        timeout_seconds: Option<u64>,
        verbose: bool,
    ) -> Result<Option<Envelope>> {
        use crate::logging::verbose_println;

        let timeout = timeout_seconds.unwrap_or(30);
        let start = std::time::Instant::now();
        let mut first_attempt = true;

        loop {
            let result = {
                let mut storage = self.storage.write().unwrap();

                if let Some(entry) = storage.get(arid) {
                    // Check if expired
                    if let Some(expires_at) = entry.expires_at
                        && Instant::now() >= expires_at
                    {
                        // Entry is expired, remove it
                        storage.remove(arid);
                        if verbose {
                            verbose_println(&format!(
                                "GET {} EXPIRED",
                                arid.ur_string()
                            ));
                        }
                        return Ok(None);
                    }

                    // Parse CBOR bytes back to Envelope
                    Envelope::try_from_cbor_data(entry.envelope_cbor.clone())
                        .ok()
                } else {
                    None
                }
            };

            if let Some(envelope) = result {
                if verbose {
                    verbose_println(&format!(
                        "GET {} OK (Memory)",
                        arid.ur_string()
                    ));
                }
                return Ok(Some(envelope));
            }

            // Not found yet
            if start.elapsed().as_secs() >= timeout {
                if verbose {
                    verbose_println(&format!(
                        "GET {} NOT_FOUND (timeout after {}s)",
                        arid.ur_string(),
                        timeout
                    ));
                }
                return Ok(None);
            }

            if first_attempt && verbose {
                verbose_println(&format!(
                    "Polling for {} (timeout: {}s)",
                    arid.ur_string(),
                    timeout
                ));
                first_attempt = false;
            } else if verbose {
                print!(".");
                use std::io::Write;
                std::io::stdout().flush().ok();
            }

            sleep(Duration::from_millis(500)).await;
        }
    }

    async fn exists(&self, arid: &ARID) -> Result<bool> {
        self.check_exists(arid)
    }
}
