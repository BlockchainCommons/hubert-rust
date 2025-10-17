use std::sync::{Arc, RwLock};

use bc_components::ARID;
use bc_envelope::Envelope;
use dcbor::CBOREncodable;
use ipfs_api_backend_hyper::{IpfsApi, IpfsClient};
use ipfs_api_prelude::request::KeyType;
use tokio::time::{Duration, Instant, sleep};

use super::{
    error::{GetError, PutError},
    value::{add_bytes, cat_bytes, pin_cid},
};
use crate::{KvStore, arid_derivation::derive_ipfs_key_name};

/// IPFS-backed key-value store using IPNS for ARID-based addressing.
///
/// This implementation uses:
/// - ARID → IPNS key name derivation (deterministic)
/// - IPFS content addressing (CID) for immutable storage
/// - IPNS for publish-once mutable names
/// - Write-once semantics (publish fails if name already exists)
///
/// # Requirements
///
/// Requires a running Kubo daemon (or compatible IPFS node) with RPC API
/// available at the configured endpoint (default: http://127.0.0.1:5001).
///
/// # Example
///
/// ```no_run
/// use bc_components::ARID;
/// use bc_envelope::Envelope;
/// use hubert::{KvStore, ipfs::IpfsKv};
///
/// # async fn example() {
/// let store = IpfsKv::new("http://127.0.0.1:5001");
/// let arid = ARID::new();
/// let envelope = Envelope::new("Hello, IPFS!");
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
pub struct IpfsKv {
    client: IpfsClient,
    key_cache: Arc<RwLock<std::collections::HashMap<String, KeyInfo>>>,
    max_envelope_size: usize,
    resolve_timeout: Duration,
    pin_content: bool,
}

#[derive(Clone, Debug)]
struct KeyInfo {
    peer_id: String,
}

impl IpfsKv {
    /// Create a new IPFS KV store with default settings.
    ///
    /// # Parameters
    ///
    /// - `rpc_url`: IPFS RPC endpoint (e.g., "http://127.0.0.1:5001")
    pub fn new(_rpc_url: &str) -> Self {
        Self {
            client: IpfsClient::default(),
            key_cache: Arc::new(RwLock::new(std::collections::HashMap::new())),
            max_envelope_size: 10 * 1024 * 1024, // 10 MB
            resolve_timeout: Duration::from_secs(30),
            pin_content: false,
        }
    }

    /// Set the maximum envelope size (default: 10 MB).
    pub fn with_max_size(mut self, size: usize) -> Self {
        self.max_envelope_size = size;
        self
    }

    /// Set the IPNS resolve timeout (default: 30 seconds).
    pub fn with_resolve_timeout(mut self, timeout: Duration) -> Self {
        self.resolve_timeout = timeout;
        self
    }

    /// Set whether to pin content (default: false).
    pub fn with_pin_content(mut self, pin: bool) -> Self {
        self.pin_content = pin;
        self
    }

    /// Get or create an IPNS key for the given ARID.
    async fn get_or_create_key(
        &self,
        arid: &ARID,
    ) -> Result<KeyInfo, PutError> {
        let key_name = derive_ipfs_key_name(arid);

        // Check cache first
        {
            let cache = self.key_cache.read().unwrap();
            if let Some(info) = cache.get(&key_name) {
                return Ok(info.clone());
            }
        }

        // List existing keys to see if it already exists
        let keys = self.client.key_list().await?;

        if let Some(key) = keys.keys.iter().find(|k| k.name == key_name) {
            let info = KeyInfo { peer_id: key.id.clone() };
            // Update cache
            self.key_cache
                .write()
                .unwrap()
                .insert(key_name, info.clone());
            return Ok(info);
        }

        // Generate new key
        let key_info =
            self.client.key_gen(&key_name, KeyType::Ed25519, 0).await?;

        let info = KeyInfo { peer_id: key_info.id };

        // Update cache
        self.key_cache
            .write()
            .unwrap()
            .insert(key_name, info.clone());

        Ok(info)
    }

    /// Check if an IPNS name is already published.
    async fn is_published(&self, peer_id: &str) -> Result<bool, PutError> {
        match self.client.name_resolve(Some(peer_id), false, false).await {
            Ok(_) => Ok(true),
            Err(e) => {
                let err_str = e.to_string();
                // IPNS name not found errors indicate unpublished name
                if err_str.contains("could not resolve name")
                    || err_str.contains("no link named")
                    || err_str.contains("not found")
                {
                    Ok(false)
                } else {
                    Err(PutError::DaemonError(err_str))
                }
            }
        }
    }

    /// Publish a CID to an IPNS name (write-once).
    async fn publish_once(
        &self,
        key_name: &str,
        peer_id: &str,
        cid: &str,
        ttl_seconds: Option<u64>,
    ) -> Result<(), PutError> {
        // Check if already published
        if self.is_published(peer_id).await? {
            return Err(PutError::AlreadyExists {
                ipns_name: peer_id.to_string(),
            });
        }

        // Convert TTL seconds to lifetime string for IPNS
        // Format: "Ns" for seconds, "Nm" for minutes, "Nh" for hours, "Nd" for
        // days
        let lifetime = ttl_seconds.map(|secs| {
            if secs < 60 {
                format!("{}s", secs)
            } else if secs < 3600 {
                format!("{}m", secs / 60)
            } else if secs < 86400 {
                format!("{}h", secs / 3600)
            } else {
                format!("{}d", secs / 86400)
            }
        });

        // Publish to IPNS
        self.client
            .name_publish(
                &format!("/ipfs/{}", cid),
                false,
                lifetime.as_deref(), // IPNS record lifetime (TTL)
                None,                // Cache TTL hint
                Some(key_name),
            )
            .await?;

        Ok(())
    }

    /// Resolve an IPNS name to a CID with polling and custom timeout.
    async fn resolve_with_retry_timeout(
        &self,
        peer_id: &str,
        timeout: Duration,
        verbose: bool,
    ) -> Result<Option<String>, GetError> {
        use crate::logging::verbose_print_dot;

        let deadline = Instant::now() + timeout;
        // Changed to 1000ms for verbose mode polling
        let poll_interval = Duration::from_millis(1000);

        loop {
            match self.client.name_resolve(Some(peer_id), false, false).await {
                Ok(res) => {
                    // Extract CID from path (e.g., "/ipfs/bafy..." ->
                    // "bafy...")
                    if let Some(cid) = res.path.strip_prefix("/ipfs/") {
                        return Ok(Some(cid.to_string()));
                    } else {
                        return Err(GetError::DaemonError(format!(
                            "unexpected IPNS path format: {}",
                            res.path
                        )));
                    }
                }
                Err(e) => {
                    let err_str = e.to_string();
                    // Check if name simply doesn't exist (not published)
                    if err_str.contains("could not resolve name")
                        || err_str.contains("no link named")
                        || err_str.contains("not found")
                    {
                        return Ok(None);
                    }

                    // Check if we've timed out
                    if Instant::now() >= deadline {
                        return Err(GetError::Timeout);
                    }

                    // Print polling dot if verbose
                    if verbose {
                        verbose_print_dot();
                    }

                    // Retry after interval (now 1000ms)
                    sleep(poll_interval).await;
                }
            }
        }
    }
}

#[async_trait::async_trait(?Send)]
impl KvStore for IpfsKv {
    async fn put(
        &self,
        arid: &ARID,
        envelope: &Envelope,
        ttl_seconds: Option<u64>,
        verbose: bool,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        self.put_impl(arid, envelope, ttl_seconds, verbose)
            .await
            .map_err(|e| {
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            })
    }

    async fn get(
        &self,
        arid: &ARID,
        timeout_seconds: Option<u64>,
        verbose: bool,
    ) -> Result<Option<Envelope>, Box<dyn std::error::Error + Send + Sync>>
    {
        self.get_impl(arid, timeout_seconds, verbose)
            .await
            .map_err(|e| {
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            })
    }

    async fn exists(
        &self,
        arid: &ARID,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        self.exists_impl(arid).await.map_err(|e| {
            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
        })
    }
}

impl IpfsKv {
    /// Internal put implementation with typed errors.
    async fn put_impl(
        &self,
        arid: &ARID,
        envelope: &Envelope,
        ttl_seconds: Option<u64>,
        verbose: bool,
    ) -> Result<String, PutError> {
        use crate::logging::{verbose_newline, verbose_println};

        if verbose {
            verbose_println("Starting IPFS put operation");
        }

        // Serialize envelope
        let bytes = envelope.to_cbor_data();

        // Check size
        if bytes.len() > self.max_envelope_size {
            return Err(PutError::EnvelopeTooLarge { size: bytes.len() });
        }

        if verbose {
            verbose_println(&format!("Envelope size: {} bytes", bytes.len()));
        }

        // Get or create IPNS key
        if verbose {
            verbose_println("Getting or creating IPNS key");
        }
        let key_info = self.get_or_create_key(arid).await?;

        let key_name = derive_ipfs_key_name(arid);

        // Add to IPFS
        if verbose {
            verbose_println("Adding content to IPFS");
        }
        let cid = add_bytes(&self.client, bytes).await?;

        if verbose {
            verbose_println(&format!("Content CID: {}", cid));
        }

        // Pin if requested
        if self.pin_content {
            if verbose {
                verbose_println("Pinning content");
            }
            pin_cid(&self.client, &cid, true).await?;
        }

        // Publish to IPNS (write-once)
        if verbose {
            verbose_println("Publishing to IPNS (write-once check)");
        }
        self.publish_once(&key_name, &key_info.peer_id, &cid, ttl_seconds)
            .await?;

        if verbose {
            verbose_println("IPFS put operation completed");
            verbose_newline();
        }

        Ok(format!("ipns://{} -> ipfs://{}", key_info.peer_id, cid))
    }

    /// Internal get implementation with typed errors.
    async fn get_impl(
        &self,
        arid: &ARID,
        timeout_seconds: Option<u64>,
        verbose: bool,
    ) -> Result<Option<Envelope>, GetError> {
        use crate::logging::{verbose_newline, verbose_println};

        if verbose {
            verbose_println("Starting IPFS get operation");
        }

        let key_name = derive_ipfs_key_name(arid);

        // Get key info from cache or daemon
        if verbose {
            verbose_println("Looking up IPNS key");
        }
        let keys = self.client.key_list().await?;

        let key = keys.keys.iter().find(|k| k.name == key_name);
        if key.is_none() {
            // Key doesn't exist, so nothing published
            if verbose {
                verbose_println("Key not found");
                verbose_newline();
            }
            return Ok(None);
        }

        let peer_id = &key.unwrap().id;

        // Resolve IPNS to CID with specified timeout
        if verbose {
            verbose_println("Resolving IPNS name (polling)");
        }
        let timeout = timeout_seconds
            .map(Duration::from_secs)
            .unwrap_or(self.resolve_timeout);
        let cid = self
            .resolve_with_retry_timeout(peer_id, timeout, verbose)
            .await?;

        if verbose {
            verbose_newline();
        }

        if cid.is_none() {
            if verbose {
                verbose_println("IPNS name not published");
                verbose_newline();
            }
            return Ok(None);
        }

        let cid = cid.unwrap();

        if verbose {
            verbose_println(&format!("Resolved to CID: {}", cid));
        }

        // Cat CID
        if verbose {
            verbose_println("Fetching content from IPFS");
        }
        let bytes = cat_bytes(&self.client, &cid).await?;

        // Deserialize envelope
        let envelope = Envelope::try_from_cbor_data(bytes)?;

        if verbose {
            verbose_println("IPFS get operation completed");
            verbose_newline();
        }

        Ok(Some(envelope))
    }

    /// Internal exists implementation with typed errors.
    async fn exists_impl(&self, arid: &ARID) -> Result<bool, GetError> {
        let key_name = derive_ipfs_key_name(arid);

        // List keys to check if key exists
        let keys = self.client.key_list().await?;

        let key = keys.keys.iter().find(|k| k.name == key_name);
        if key.is_none() {
            return Ok(false);
        }

        let peer_id = &key.unwrap().id;

        // Check if published (quick resolve)
        match self.client.name_resolve(Some(peer_id), false, false).await {
            Ok(_) => Ok(true),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("could not resolve name")
                    || err_str.contains("no link named")
                    || err_str.contains("not found")
                {
                    Ok(false)
                } else {
                    Err(GetError::DaemonError(err_str))
                }
            }
        }
    }
}
