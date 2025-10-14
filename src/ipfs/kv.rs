use std::sync::{Arc, RwLock};

use bc_components::ARID;
use bc_envelope::Envelope;
use ipfs_api_backend_hyper::{IpfsApi, IpfsClient};
use ipfs_api_prelude::request::KeyType;
use tokio::time::{Duration, Instant, sleep};

use super::{
    arid_derivation::derive_key_name,
    error::{GetError, PutError},
    value::{
        add_bytes, cat_bytes, deserialize_envelope, pin_cid, serialize_envelope,
    },
};
use crate::KvStore;

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
/// use hubert::ipfs::IpfsKv;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let store = IpfsKv::new("http://127.0.0.1:5001");
/// let arid = ARID::new();
/// let envelope = Envelope::new("Hello, IPFS!");
///
/// // Put envelope (write-once)
/// store.put(&arid, &envelope).await?;
///
/// // Get envelope
/// if let Some(retrieved) = store.get(&arid).await? {
///     assert_eq!(retrieved, envelope);
/// }
/// # Ok(())
/// # }
/// ```
pub struct IpfsKv {
    client: IpfsClient,
    key_cache: Arc<RwLock<std::collections::HashMap<String, KeyInfo>>>,
    max_envelope_size: usize,
    resolve_timeout: Duration,
    resolve_poll_interval: Duration,
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
            resolve_poll_interval: Duration::from_millis(500),
            pin_content: true,
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

    /// Set whether to pin content (default: true).
    pub fn with_pin_content(mut self, pin: bool) -> Self {
        self.pin_content = pin;
        self
    }

    /// Get or create an IPNS key for the given ARID.
    async fn get_or_create_key(
        &self,
        arid: &ARID,
    ) -> Result<KeyInfo, PutError> {
        let key_name = derive_key_name(arid);

        // Check cache first
        {
            let cache = self.key_cache.read().unwrap();
            if let Some(info) = cache.get(&key_name) {
                return Ok(info.clone());
            }
        }

        // List existing keys to see if it already exists
        let keys = self
            .client
            .key_list()
            .await
            .map_err(|e| PutError::DaemonError(e.to_string()))?;

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
        let key_info = self
            .client
            .key_gen(&key_name, KeyType::Ed25519, 0)
            .await
            .map_err(|e| PutError::DaemonError(e.to_string()))?;

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
    ) -> Result<(), PutError> {
        // Check if already published
        if self.is_published(peer_id).await? {
            return Err(PutError::AlreadyExists {
                ipns_name: peer_id.to_string(),
            });
        }

        // Publish to IPNS
        self.client
            .name_publish(
                &format!("/ipfs/{}", cid),
                false,
                None,
                None,
                Some(key_name),
            )
            .await
            .map_err(|e| PutError::DaemonError(e.to_string()))?;

        Ok(())
    }

    /// Resolve an IPNS name to a CID with polling.
    async fn resolve_with_retry(
        &self,
        peer_id: &str,
    ) -> Result<Option<String>, GetError> {
        let deadline = Instant::now() + self.resolve_timeout;

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

                    // Retry after interval
                    sleep(self.resolve_poll_interval).await;
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
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // Serialize envelope
        let bytes = serialize_envelope(envelope).map_err(|e| {
            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
        })?;

        // Check size
        if bytes.len() > self.max_envelope_size {
            return Err(Box::new(PutError::EnvelopeTooLarge {
                size: bytes.len(),
            })
                as Box<dyn std::error::Error + Send + Sync>);
        }

        // Get or create IPNS key
        let key_info = self.get_or_create_key(arid).await.map_err(|e| {
            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
        })?;

        let key_name = derive_key_name(arid);

        // Add to IPFS
        let cid = add_bytes(&self.client, bytes).await.map_err(|e| {
            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
        })?;

        // Pin if requested
        if self.pin_content {
            pin_cid(&self.client, &cid, true).await.map_err(|e| {
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            })?;
        }

        // Publish to IPNS (write-once)
        self.publish_once(&key_name, &key_info.peer_id, &cid)
            .await
            .map_err(|e| {
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            })?;

        Ok(format!("ipns://{} -> ipfs://{}", key_info.peer_id, cid))
    }

    async fn get(
        &self,
        arid: &ARID,
    ) -> Result<Option<Envelope>, Box<dyn std::error::Error + Send + Sync>>
    {
        let key_name = derive_key_name(arid);

        // Get key info from cache or daemon
        let keys = self.client.key_list().await.map_err(|e| {
            Box::new(GetError::DaemonError(e.to_string()))
                as Box<dyn std::error::Error + Send + Sync>
        })?;

        let key = keys.keys.iter().find(|k| k.name == key_name);
        if key.is_none() {
            // Key doesn't exist, so nothing published
            return Ok(None);
        }

        let peer_id = &key.unwrap().id;

        // Resolve IPNS to CID
        let cid = self.resolve_with_retry(peer_id).await.map_err(|e| {
            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
        })?;

        if cid.is_none() {
            return Ok(None);
        }

        let cid = cid.unwrap();

        // Cat CID
        let bytes = cat_bytes(&self.client, &cid).await.map_err(|e| {
            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
        })?;

        // Deserialize envelope
        let envelope = deserialize_envelope(&bytes).map_err(|e| {
            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
        })?;

        Ok(Some(envelope))
    }

    async fn exists(
        &self,
        arid: &ARID,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let key_name = derive_key_name(arid);

        // List keys to check if key exists
        let keys = self.client.key_list().await.map_err(|e| {
            Box::new(GetError::DaemonError(e.to_string()))
                as Box<dyn std::error::Error + Send + Sync>
        })?;

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
                    Err(Box::new(GetError::DaemonError(err_str))
                        as Box<dyn std::error::Error + Send + Sync>)
                }
            }
        }
    }
}
