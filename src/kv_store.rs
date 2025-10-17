use std::error::Error;

use bc_components::ARID;
use bc_envelope::Envelope;

/// Unified trait for key-value storage backends using ARID-based addressing.
///
/// All implementations provide write-once semantics: once an envelope is stored
/// at an ARID, subsequent attempts to write to the same ARID will fail with an
/// `AlreadyExists` error.
///
/// # Security Model
///
/// - ARID holder can read (by deriving storage key)
/// - ARID creator can write once (by deriving storage key)
/// - Storage networks see only derived keys, never ARIDs themselves
/// - ARIDs shared only via secure channels (GSTP, Signal, QR codes)
///
/// # Implementations
///
/// - `MainlineDhtKv`: Fast, lightweight DHT storage (≤1 KB messages)
/// - `IpfsKv`: Large capacity, content-addressed storage (up to 10 MB messages)
/// - `HybridKv`: Automatic optimization by size, combining DHT speed with IPFS
///   capacity
///
/// # Thread Safety
///
/// The `KvStore` trait requires `Send + Sync`, meaning implementations can be
/// safely shared across threads. However, the futures returned by async methods
/// are **not required to be `Send`** (note the `?Send` bound on `async_trait`).
///
/// **What this means in practice:**
///
/// - ✓ You can share a `KvStore` instance across threads
/// - ✓ You can call methods and await them on any thread
/// - ✓ Multiple threads can perform concurrent operations
/// - ✗ You cannot move an in-flight future to another thread
///
/// **Working pattern:**
/// ```no_run
/// # use hubert::{ipfs::IpfsKv, KvStore};
/// # use bc_components::ARID;
/// # use bc_envelope::Envelope;
/// use std::sync::Arc;
///
/// # async fn example() {
/// let store = Arc::new(IpfsKv::new("http://127.0.0.1:5001"));
///
/// // Spawn threads that each do async work locally
/// let store1 = Arc::clone(&store);
/// let handle = std::thread::spawn(move || {
///     tokio::runtime::Runtime::new().unwrap().block_on(async {
///         let arid = ARID::new();
///         let env = Envelope::new("data");
///         store1.put(&arid, &env, None, false).await
///     })
/// });
/// # }
/// ```
///
/// **Non-working pattern:**
/// ```compile_fail
/// # use hubert::{ipfs::IpfsKv, KvStore};
/// # use bc_components::ARID;
/// # use bc_envelope::Envelope;
/// # async fn example() {
/// let store = IpfsKv::new("http://127.0.0.1:5001");
/// let arid = ARID::new();
/// let env = Envelope::new("data");
///
/// // ERROR: Cannot spawn !Send future across threads
/// tokio::spawn(async move {
///     store.put(&arid, &env, None, false).await
/// });
/// # }
/// ```
///
/// This limitation comes from underlying network client libraries and is
/// typical for async I/O code. It does not prevent concurrent operations - each
/// thread simply needs to `.await` its own futures locally.
#[async_trait::async_trait(?Send)]
pub trait KvStore: Send + Sync {
    /// Store an envelope at the given ARID.
    ///
    /// # Write-Once Semantics
    ///
    /// This operation will fail if the ARID already exists. The
    /// implementation must check for existence before writing and return an
    /// appropriate error if the key is already present.
    ///
    /// # Parameters
    ///
    /// - `arid`: Cryptographic identifier for this storage location
    /// - `envelope`: The envelope to store
    /// - `ttl_seconds`: Optional time-to-live in seconds. After this time, the
    ///   envelope may be removed from storage.
    ///   - **Mainline DHT**: Ignored (no TTL support)
    ///   - **IPFS**: Used as IPNS record lifetime (default: 24h if None)
    ///   - **Server**: Clamped to max_ttl if exceeded; uses max_ttl if None.
    ///     All entries expire (hubert is for coordination, not long-term
    ///     storage).
    /// - `verbose`: If true, log operations with timestamps
    ///
    /// # Returns
    ///
    /// A receipt containing storage metadata on success, or an error if:
    /// - The ARID already exists (AlreadyExists)
    /// - The envelope is too large for this backend
    /// - Network operation fails
    /// - Serialization fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use hubert::KvStore;
    /// # use bc_components::ARID;
    /// # use bc_envelope::Envelope;
    /// # async fn example(store: &impl hubert::KvStore) {
    /// let arid = ARID::new();
    /// let envelope = Envelope::new("Hello, Hubert!");
    ///
    /// // Store without TTL
    /// let receipt = store.put(&arid, &envelope, None, false).await.unwrap();
    ///
    /// // Store with 1 hour TTL and verbose logging
    /// let arid2 = ARID::new();
    /// let receipt2 = store
    ///     .put(&arid2, &envelope, Some(3600), true)
    ///     .await
    ///     .unwrap();
    /// println!("Stored at: {}", receipt2);
    /// # }
    /// ```
    async fn put(
        &self,
        arid: &ARID,
        envelope: &Envelope,
        ttl_seconds: Option<u64>,
        verbose: bool,
    ) -> Result<String, Box<dyn Error + Send + Sync>>;

    /// Retrieve an envelope for the given ARID.
    ///
    /// Polls the storage backend until the envelope becomes available or the
    /// timeout is reached. This is useful for coordinating between parties
    /// where one party puts data and another polls for it.
    ///
    /// # Parameters
    ///
    /// - `arid`: The ARID to look up
    /// - `timeout_seconds`: Maximum time to wait for the envelope to appear. If
    ///   `None`, uses a backend-specific default (typically 30 seconds). After
    ///   timeout, returns `Ok(None)` rather than continuing to poll.
    /// - `verbose`: If true, log operations with timestamps and print polling
    ///   dots
    ///
    /// # Returns
    ///
    /// - `Ok(Some(envelope))` if found within the timeout
    /// - `Ok(None)` if not found after timeout expires
    /// - `Err(_)` on network or deserialization errors
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use hubert::KvStore;
    /// # use bc_components::ARID;
    /// # async fn example(store: &impl hubert::KvStore, arid: &ARID) {
    /// // Wait up to 10 seconds for envelope to appear with verbose logging
    /// match store.get(arid, Some(10), true).await.unwrap() {
    ///     Some(envelope) => println!("Found: {}", envelope),
    ///     None => println!("Not found within timeout"),
    /// }
    /// # }
    /// ```
    async fn get(
        &self,
        arid: &ARID,
        timeout_seconds: Option<u64>,
        verbose: bool,
    ) -> Result<Option<Envelope>, Box<dyn Error + Send + Sync>>;

    /// Check if an envelope exists at the given ARID.
    ///
    /// # Parameters
    ///
    /// - `arid`: The ARID to check
    ///
    /// # Returns
    ///
    /// - `Ok(true)` if the ARID exists
    /// - `Ok(false)` if the ARID does not exist
    /// - `Err(_)` on network errors
    ///
    /// # Implementation Note
    ///
    /// For hybrid storage, this only checks the DHT layer. Reference envelopes
    /// count as existing even if the referenced IPFS content is not available.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use hubert::KvStore;
    /// # use bc_components::ARID;
    /// # async fn example(store: &impl hubert::KvStore, arid: &ARID) {
    /// if store.exists(arid).await.unwrap() {
    ///     println!("ARID already used");
    /// } else {
    ///     println!("ARID available");
    /// }
    /// # }
    /// ```
    async fn exists(
        &self,
        arid: &ARID,
    ) -> Result<bool, Box<dyn Error + Send + Sync>>;
}
