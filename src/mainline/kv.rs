use bc_components::ARID;
use bc_envelope::Envelope;
use bc_ur::UREncodable;
use dcbor::CBOREncodable;
use mainline::{Dht, MutableItem, SigningKey};

use super::error::Error as MainlineError;
use crate::{Error, KvStore, Result, arid_derivation::derive_mainline_key};

/// Mainline DHT-backed key-value store using ARID-based addressing.
///
/// This implementation uses:
/// - ARID → ed25519 signing key derivation (deterministic)
/// - BEP-44 mutable storage (fixed location based on pubkey)
/// - Mainline DHT (BitTorrent DHT) for decentralized storage
/// - Write-once semantics (seq=1, put fails if already exists)
/// - Maximum value size: 1000 bytes (DHT protocol limit)
///
/// # Storage Model
///
/// Uses BEP-44 mutable items where:
/// - Public key derived from ARID (deterministic ed25519)
/// - Sequence number starts at 1 (write-once)
/// - Optional salt for namespace separation
/// - Location fixed by pubkey (not content hash)
///
/// # Requirements
///
/// No external daemon required - the DHT client runs embedded.
///
/// # Size Limits
///
/// The Mainline DHT has a practical limit of ~1KB per value. For larger
/// envelopes, use `IpfsKv` or `HybridKv` instead.
///
/// # Example
///
/// ```no_run
/// use bc_components::ARID;
/// use bc_envelope::Envelope;
/// use hubert::{KvStore, mainline::MainlineDhtKv};
///
/// # async fn example() {
/// let store = MainlineDhtKv::new().await.unwrap();
/// let arid = ARID::new();
/// let envelope = Envelope::new("Small message");
///
/// // Put envelope (write-once)
/// store.put(&arid, &envelope, None, false).await.unwrap();
///
/// // Get envelope with verbose logging
/// if let Some(retrieved) = store.get(&arid, None, true).await.unwrap() {
///     assert_eq!(retrieved, envelope);
/// }
/// # }
/// ```
pub struct MainlineDhtKv {
    dht: mainline::async_dht::AsyncDht,
    max_value_size: usize,
    salt: Option<Vec<u8>>,
}

impl MainlineDhtKv {
    /// Create a new Mainline DHT KV store with default settings.
    pub async fn new() -> Result<Self> {
        let dht = Dht::client().map_err(MainlineError::from)?.as_async();

        // Wait for bootstrap
        dht.bootstrapped().await;

        Ok(Self {
            dht,
            max_value_size: 1000, // DHT protocol limit
            salt: None,           // No salt by default
        })
    }

    /// Set the maximum value size (default: 1000 bytes).
    ///
    /// Note: Values larger than ~1KB may not be reliably stored in the DHT.
    pub fn with_max_size(mut self, size: usize) -> Self {
        self.max_value_size = size;
        self
    }

    /// Set a salt for namespace separation.
    ///
    /// Different salts will create separate namespaces for the same ARID.
    pub fn with_salt(mut self, salt: Vec<u8>) -> Self {
        self.salt = Some(salt);
        self
    }

    /// Derive an ed25519 signing key from an ARID.
    ///
    /// Uses the ARID-derived key material extended to 32 bytes for ed25519.
    fn derive_signing_key(arid: &ARID) -> SigningKey {
        let key_hex = derive_mainline_key(arid);
        let key_bytes = hex::decode(&key_hex).expect("valid hex from derive");

        // Extend to 32 bytes if needed (ARID gives us 20, we need 32)
        let mut seed = [0u8; 32];
        seed[..20].copy_from_slice(&key_bytes[..20]);
        // Use simple derivation for remaining 12 bytes
        for i in 20..32 {
            seed[i] = key_bytes[i % 20].wrapping_mul(i as u8);
        }

        SigningKey::from_bytes(&seed)
    }
}

#[async_trait::async_trait(?Send)]
impl KvStore for MainlineDhtKv {
    async fn put(
        &self,
        arid: &ARID,
        envelope: &Envelope,
        ttl_seconds: Option<u64>,
        verbose: bool,
    ) -> Result<String> {
        self.put_impl(arid, envelope, ttl_seconds, verbose).await
    }

    async fn get(
        &self,
        arid: &ARID,
        timeout_seconds: Option<u64>,
        verbose: bool,
    ) -> Result<Option<Envelope>> {
        // Polls DHT with specified timeout
        self.get_impl(arid, timeout_seconds, verbose).await
    }

    async fn exists(&self, arid: &ARID) -> Result<bool> {
        self.exists_impl(arid).await
    }
}

impl MainlineDhtKv {
    /// Internal put implementation with typed errors.
    async fn put_impl(
        &self,
        arid: &ARID,
        envelope: &Envelope,
        _ttl_seconds: Option<u64>, // Ignored - DHT has no TTL support
        verbose: bool,
    ) -> Result<String> {
        use crate::logging::verbose_println;

        if verbose {
            verbose_println("Starting Mainline DHT put operation");
        }

        // Serialize envelope
        let bytes = envelope.to_cbor_data();

        // Check size
        if bytes.len() > self.max_value_size {
            return Err(
                MainlineError::ValueTooLarge { size: bytes.len() }.into()
            );
        }

        if verbose {
            verbose_println(&format!("Envelope size: {} bytes", bytes.len()));
        }

        // Derive signing key from ARID
        if verbose {
            verbose_println("Deriving DHT signing key from ARID");
        }
        let signing_key = Self::derive_signing_key(arid);
        let pubkey = signing_key.verifying_key().to_bytes();
        let salt_opt = self.salt.as_deref();

        // Check if already exists (write-once semantics)
        if verbose {
            verbose_println("Checking for existing value (write-once check)");
        }
        if self
            .dht
            .get_mutable_most_recent(&pubkey, salt_opt)
            .await
            .is_some()
        {
            return Err(Error::AlreadyExists { arid: arid.ur_string() });
        }

        // Create mutable item with seq=1 (first write)
        if verbose {
            verbose_println("Creating mutable DHT item");
        }
        let item = MutableItem::new(signing_key, &bytes, 1, salt_opt);

        // Put to DHT (no CAS since we verified it doesn't exist)
        if verbose {
            verbose_println("Putting value to DHT");
        }
        self.dht
            .put_mutable(item, None)
            .await
            .map_err(MainlineError::from)?;

        if verbose {
            verbose_println("Mainline DHT put operation completed");
        }

        Ok(format!("dht://{}", hex::encode(pubkey)))
    }

    /// Internal get implementation with typed errors.
    async fn get_impl(
        &self,
        arid: &ARID,
        timeout_seconds: Option<u64>,
        verbose: bool,
    ) -> Result<Option<Envelope>> {
        use tokio::time::{Duration, Instant, sleep};

        use crate::logging::{
            verbose_newline, verbose_print_dot, verbose_println,
        };

        if verbose {
            verbose_println("Starting Mainline DHT get operation");
        }

        // Derive public key from ARID
        if verbose {
            verbose_println("Deriving DHT public key from ARID");
        }
        let signing_key = Self::derive_signing_key(arid);
        let pubkey = signing_key.verifying_key().to_bytes();
        let salt_opt = self.salt.as_deref();

        let timeout = timeout_seconds.unwrap_or(30); // Default 30 seconds
        let deadline = Instant::now() + Duration::from_secs(timeout);
        // Changed to 1000ms for verbose mode polling
        let poll_interval = Duration::from_millis(1000);

        if verbose {
            verbose_println("Polling DHT for value");
        }

        loop {
            // Get most recent mutable item
            let item =
                self.dht.get_mutable_most_recent(&pubkey, salt_opt).await;

            if let Some(mutable_item) = item {
                if verbose {
                    verbose_newline();
                    verbose_println("Value found in DHT");
                }
                // Deserialize envelope from value
                let envelope = Envelope::try_from_cbor_data(
                    mutable_item.value().to_vec(),
                )?;

                if verbose {
                    verbose_println("Mainline DHT get operation completed");
                }

                return Ok(Some(envelope));
            }

            // Not found yet - check if we should keep polling
            if Instant::now() >= deadline {
                // Timeout reached
                if verbose {
                    verbose_newline();
                    verbose_println("Timeout reached, value not found");
                }
                return Ok(None);
            }

            // Print polling dot if verbose
            if verbose {
                verbose_print_dot();
            }

            // Wait before retrying (now 1000ms)
            sleep(poll_interval).await;
        }
    }

    /// Internal exists implementation with typed errors.
    async fn exists_impl(&self, arid: &ARID) -> Result<bool> {
        let signing_key = Self::derive_signing_key(arid);
        let pubkey = signing_key.verifying_key().to_bytes();
        let salt_opt = self.salt.as_deref();

        // Check if mutable item exists
        let item = self.dht.get_mutable_most_recent(&pubkey, salt_opt).await;
        Ok(item.is_some())
    }
}
