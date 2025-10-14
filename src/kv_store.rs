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
///         store1.put(&arid, &env).await
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
///     store.put(&arid, &env).await
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
    /// This operation will fail if the ARID already exists in storage. The
    /// implementation must check for existence before writing and return an
    /// appropriate error if the key is already present.
    ///
    /// # Parameters
    ///
    /// - `arid`: Cryptographic identifier for this storage location
    /// - `envelope`: The envelope to store
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
    /// let receipt = store.put(&arid, &envelope).await.unwrap();
    /// println!("Stored at: {}", receipt);
    /// # }
    /// ```
    async fn put(
        &self,
        arid: &ARID,
        envelope: &Envelope,
    ) -> Result<String, Box<dyn Error + Send + Sync>>;

    /// Retrieve an envelope for the given ARID.
    ///
    /// # Parameters
    ///
    /// - `arid`: The ARID to look up
    ///
    /// # Returns
    ///
    /// - `Ok(Some(envelope))` if found
    /// - `Ok(None)` if not found
    /// - `Err(_)` on network or deserialization errors
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use hubert::KvStore;
    /// # use bc_components::ARID;
    /// # async fn example(store: &impl hubert::KvStore, arid: &ARID) {
    /// match store.get(arid).await.unwrap() {
    ///     Some(envelope) => println!("Found: {}", envelope),
    ///     None => println!("Not found"),
    /// }
    /// # }
    /// ```
    async fn get(
        &self,
        arid: &ARID,
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
