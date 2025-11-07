use bc_components::ARID;
use bc_envelope::Envelope;
use bc_ur::prelude::*;

use super::{
    Error as HybridError,
    reference::{
        create_reference_envelope, extract_reference_arid,
        is_reference_envelope,
    },
};
use crate::{
    KvStore, Result, arid_derivation::derive_reference_encryption_key,
    ipfs::IpfsKv, logging::verbose_println, mainline::MainlineDhtKv,
};

/// Hybrid storage layer combining Mainline DHT and IPFS.
///
/// Automatically optimizes storage based on envelope size:
/// - **Small envelopes (≤1000 bytes)**: Stored directly in DHT
/// - **Large envelopes (>1000 bytes)**: Reference in DHT → actual envelope in
///   IPFS
///
/// This provides the best of both worlds:
/// - Fast lookups for small messages via DHT
/// - Large capacity for big messages via IPFS
/// - Transparent indirection handled automatically
///
/// # Requirements
///
/// - No external daemon for DHT (embedded client)
/// - Requires Kubo daemon for IPFS (http://127.0.0.1:5001)
///
/// # Example
///
/// ```no_run
/// use bc_components::ARID;
/// use bc_envelope::Envelope;
/// use hubert::{KvStore, hybrid::HybridKv};
///
/// # async fn example() {
/// let store = HybridKv::new("http://127.0.0.1:5001").await.unwrap();
///
/// // Small envelope → DHT only
/// let arid1 = ARID::new();
/// let small = Envelope::new("Small message");
/// store.put(&arid1, &small, None, false).await.unwrap();
///
/// // Large envelope → DHT reference + IPFS
/// let arid2 = ARID::new();
/// let large = Envelope::new("x".repeat(2000));
/// store.put(&arid2, &large, None, false).await.unwrap();
///
/// // Get works the same for both
/// let _retrieved1 = store.get(&arid1, None, false).await.unwrap();
/// let _retrieved2 = store.get(&arid2, None, false).await.unwrap();
/// # }
/// ```
pub struct HybridKv {
    dht: MainlineDhtKv,
    ipfs: IpfsKv,
    dht_size_limit: usize,
}

impl HybridKv {
    /// Create a new Hybrid KV store with default settings.
    ///
    /// # Parameters
    ///
    /// - `ipfs_rpc_url`: IPFS RPC endpoint (e.g., "http://127.0.0.1:5001")
    ///
    /// # Errors
    ///
    /// Returns error if DHT client initialization fails.
    pub async fn new(ipfs_rpc_url: &str) -> Result<Self> {
        let dht = MainlineDhtKv::new().await?;
        let ipfs = IpfsKv::new(ipfs_rpc_url);

        Ok(Self {
            dht,
            ipfs,
            dht_size_limit: 1000, // Conservative DHT limit
        })
    }

    /// Set custom DHT size limit (default: 1000 bytes).
    ///
    /// Envelopes larger than this will use IPFS indirection.
    pub fn with_dht_size_limit(mut self, limit: usize) -> Self {
        self.dht_size_limit = limit;
        self
    }

    /// Set whether to pin content in IPFS (default: false).
    ///
    /// Only affects envelopes stored in IPFS (when larger than DHT limit).
    pub fn with_pin_content(mut self, pin: bool) -> Self {
        self.ipfs = self.ipfs.with_pin_content(pin);
        self
    }

    /// Check if an envelope fits in the DHT.
    fn fits_in_dht(&self, envelope: &Envelope) -> bool {
        let serialized = envelope.tagged_cbor().to_cbor_data();
        serialized.len() <= self.dht_size_limit
    }

    /// Put an envelope using hybrid storage logic.
    async fn put_impl(
        &self,
        arid: &ARID,
        envelope: &Envelope,
        ttl_seconds: Option<u64>,
        verbose: bool,
    ) -> Result<String> {
        // Check if it fits in DHT
        if self.fits_in_dht(envelope) {
            // Store directly in DHT
            if verbose {
                verbose_println(&format!(
                    "Storing envelope in DHT (size ≤ {} bytes)",
                    self.dht_size_limit
                ));
            }
            self.dht.put(arid, envelope, ttl_seconds, verbose).await?;
            Ok(format!("Stored in DHT at ARID: {}", arid.ur_string()))
        } else {
            // Use IPFS with DHT reference
            if verbose {
                verbose_println(
                    "Envelope too large for DHT, using IPFS indirection",
                );
            }

            // 1. Store actual envelope in IPFS with a new ARID
            let reference_arid = ARID::new();
            if verbose {
                verbose_println(&format!(
                    "Storing actual envelope in IPFS with reference ARID: {}",
                    reference_arid.ur_string()
                ));
            }
            self.ipfs
                .put(&reference_arid, envelope, ttl_seconds, verbose)
                .await?;

            // 2. Create reference envelope
            let envelope_size = envelope.tagged_cbor().to_cbor_data().len();
            let reference =
                create_reference_envelope(&reference_arid, envelope_size);

            // 3. Encrypt reference envelope with key derived from original ARID
            let encryption_key = derive_reference_encryption_key(arid);
            let encrypted_reference = reference.encrypt(&encryption_key);

            if verbose {
                verbose_println(
                    "Encrypted reference envelope to hide IPFS ARID",
                );
            }

            // 4. Store encrypted reference in DHT
            if verbose {
                verbose_println(
                    "Storing encrypted reference envelope in DHT at original ARID",
                );
            }
            self.dht
                .put(arid, &encrypted_reference, ttl_seconds, verbose)
                .await?;

            Ok(format!(
                "Stored in IPFS (ref: {}) via DHT at ARID: {}",
                reference_arid.ur_string(),
                arid.ur_string()
            ))
        }
    }

    /// Get an envelope using hybrid storage logic.
    async fn get_impl(
        &self,
        arid: &ARID,
        timeout_seconds: Option<u64>,
        verbose: bool,
    ) -> Result<Option<Envelope>> {
        // 1. Try to get from DHT
        let dht_envelope = self.dht.get(arid, timeout_seconds, verbose).await?;

        match dht_envelope {
            None => Ok(None),
            Some(envelope) => {
                // 2. Check if envelope is encrypted
                if !envelope.is_encrypted() {
                    // Not encrypted, treat as direct payload
                    if verbose {
                        verbose_println(
                            "Envelope not encrypted, treating as direct payload",
                        );
                    }
                    return Ok(Some(envelope));
                }

                // 3. Attempt to decrypt the envelope with key derived from ARID
                let encryption_key = derive_reference_encryption_key(arid);
                let decrypted_envelope = match envelope.decrypt(&encryption_key)
                {
                    Ok(decrypted) => {
                        if verbose {
                            verbose_println(
                                "Successfully decrypted reference envelope",
                            );
                        }
                        decrypted
                    }
                    Err(_) => {
                        // Decryption with our reference key failed - envelope
                        // is encrypted with a different key (e.g., user's own
                        // encryption key). Treat as actual payload.
                        if verbose {
                            verbose_println(
                                "Encrypted with different key, treating as direct payload",
                            );
                        }
                        return Ok(Some(envelope));
                    }
                };

                // 4. Check if the decrypted envelope is a reference envelope
                if is_reference_envelope(&decrypted_envelope) {
                    if verbose {
                        verbose_println(
                            "Found reference envelope, fetching actual envelope from IPFS",
                        );
                    }

                    // 5. Extract reference ARID
                    let reference_arid =
                        extract_reference_arid(&decrypted_envelope)?;

                    if verbose {
                        verbose_println(&format!(
                            "Reference ARID: {}",
                            reference_arid.ur_string()
                        ));
                    }

                    // 6. Retrieve actual envelope from IPFS
                    let ipfs_envelope = self
                        .ipfs
                        .get(&reference_arid, timeout_seconds, verbose)
                        .await?;

                    match ipfs_envelope {
                        Some(actual) => {
                            if verbose {
                                verbose_println(
                                    "Successfully retrieved actual envelope from IPFS",
                                );
                            }
                            Ok(Some(actual))
                        }
                        None => Err(HybridError::ContentNotFound.into()),
                    }
                } else {
                    // Successfully decrypted with our reference key, but it's
                    // not a valid reference envelope. This indicates data
                    // corruption or malicious data, since we only encrypt
                    // reference envelopes with this key.
                    Err(HybridError::InvalidDecryptedReference.into())
                }
            }
        }
    }
}

#[async_trait::async_trait(?Send)]
impl KvStore for HybridKv {
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
        self.get_impl(arid, timeout_seconds, verbose).await
    }

    async fn exists(&self, arid: &ARID) -> Result<bool> {
        // Check DHT only (references count as existing)
        self.dht.exists(arid).await
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_placeholder() {
        // Unit tests require async runtime
        // See integration tests in tests/test_hybrid_kv.rs
    }
}
