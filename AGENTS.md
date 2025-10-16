# Hubert: Distributed Key-Value Store APIs

This document outlines architectures and implementation plans for using BitTorrent mainline DHT and IPFS as **write-once** key-value stores where putters choose their own keys.

## Design Philosophy

**Write-Once Semantics:**
- Each ARID key is written exactly once by the putter
- No support for updates, versioning, or multiple values per ARID
- Putter distributes ARID to getters via external means (out-of-band)
- Getters independently derive storage-layer keys from the ARID
- Simplified API with no CAS, sequence numbers, or conflict resolution

**Envelope-Based Values:**
- All values are Gordian Envelopes (`bc_envelope::Envelope`)
- Envelopes provide structured, extensible data format
- Native support for encryption, compression, signatures, and elision
- Deterministic dCBOR serialization for network transport
- Intrinsic Merkle digest tree for integrity verification
- Serialization: `envelope.tagged_cbor().to_cbor_data()` → bytes
- Deserialization: `Envelope::try_from_cbor_data(bytes)` → envelope

**Key Distribution Model:**
- Putter generates/chooses an ARID
- Putter creates an Envelope with data
- Putter writes envelope once to DHT/IPFS using ARID
- Putter shares ARID with getters (QR code, envelope, secure channel, etc.)
- Getters use same ARID to derive identical storage key and retrieve envelope
- ARID acts as lookup capability; application-layer encryption (if used) provides access control

## 1. BitTorrent Mainline DHT Key-Value Store

### 1.1 Architecture

#### Core Concepts

The BitTorrent mainline DHT (BEP-5/BEP-44) provides two storage modes:

1. **Immutable Storage** (BEP-44 immutable items)
   - Key: SHA-1 hash of the value (deterministic)
   - Value: Serialized Envelope as bytes (≤1 KiB after bencode encoding)
   - Immutable after storage
   - No authentication required

2. **Mutable Storage** (BEP-44 mutable items)
   - Key: Derived from ed25519 public key + optional salt
   - Value: Serialized Envelope as bytes (≤1 KiB after bencode encoding)
   - Updatable via sequence numbers (CAS semantics)
   - Signed with ed25519 private key
   - **Used for write-once with chosen keys** (updates not utilized)

#### Key Selection Strategy

For **write-once putter-chosen keys**, use mutable storage format without updates:

- **User-provided ARID** (32-byte identifier from `bc_components::ARID`)
- **HKDF-based key stretching** via `bc_crypto::hkdf_hmac_sha256`
- **Target DHT key** = SHA-1(pubkey || salt)
- **Write-once**: Always use seq=1, no subsequent updates
- **No DHT salt**: Single value per ARID (salt always None)

ARID-to-ed25519 derivation:
1. Input: `ARID` (32 bytes, from `bc_components::ARID`)
2. Salt: Context-specific constant (`b"hubert-mainline-dht-v1"`)
3. HKDF: `hkdf_hmac_sha256(arid.as_bytes(), salt, 32)` → ed25519 seed
4. SigningKey: `mainline::SigningKey::from_bytes(&seed)`
5. Publish with seq=1, salt=None (write-once)

This ensures:
- Deterministic key derivation from ARID
- Same ARID always resolves to same DHT location
- No complexity from updates or versioning
- Simplified put operation (no read-before-write)

#### API Design

```rust
use bc_components::ARID;
use bc_envelope::Envelope;

pub struct MainlineDhtKv {
    dht: AsyncDht,
    hkdf_salt: Vec<u8>,  // Context-specific salt for HKDF
}

pub struct PutOptions {
    /// Timeout for put operation
    pub timeout: Duration,
}

pub struct GetOptions {
    /// Poll until found or timeout
    pub poll_timeout: Duration,
    /// Interval between poll attempts
    pub poll_interval: Duration,
}

impl MainlineDhtKv {
    /// Create a new Mainline DHT KV store with custom HKDF salt
    pub fn new(dht: AsyncDht, hkdf_salt: impl AsRef<[u8]>) -> Self;

    /// Create with default HKDF salt ("hubert-mainline-dht-v1")
    pub fn with_default_salt(dht: AsyncDht) -> Self;

    /// Put envelope with ARID-based key (write-once)
    /// Serializes envelope to dCBOR and stores in DHT
    /// Returns error if key already exists (seq > 0)
    pub async fn put(
        &self,
        arid: &ARID,
        envelope: &Envelope,
        options: PutOptions,
    ) -> Result<PutReceipt, PutError>;

    /// Get envelope for ARID-based key
    /// Retrieves bytes from DHT and deserializes to Envelope
    pub async fn get(
        &self,
        arid: &ARID,
        options: GetOptions,
    ) -> Result<Option<Envelope>, GetError>;

    /// Check if ARID key exists (without fetching envelope)
    pub async fn exists(
        &self,
        arid: &ARID,
        options: GetOptions,
    ) -> Result<bool, GetError>;

    /// Derive ed25519 signing key from ARID (exposed for verification)
    pub fn derive_signing_key(&self, arid: &ARID) -> SigningKey;

    /// Get the public key for an ARID (for diagnostics)
    pub fn derive_public_key(&self, arid: &ARID) -> [u8; 32];
}

pub struct PutReceipt {
    pub target_id: Id,     // DHT lookup key
    pub pubkey: [u8; 32],  // Derived public key
    pub arid: ARID,        // Original ARID used
    pub envelope_size: usize, // Size of serialized envelope
}

#[derive(Debug, thiserror::Error)]
pub enum PutError {
    #[error("ARID already exists with sequence number {current_seq}")]
    AlreadyExists { current_seq: i64 },

    #[error("Envelope size {size} exceeds limit of {limit} bytes after bencode")]
    EnvelopeTooLarge { size: usize, limit: usize },

    #[error("DHT network error: {0}")]
    NetworkError(String),

    #[error("Operation timed out")]
    Timeout,

    #[error("Envelope serialization error: {0}")]
    EnvelopeError(#[from] bc_envelope::Error),

    #[error("CBOR error: {0}")]

#### Phase 2: Basic Put/Get Operations

3. **Put Implementation** (`mainline/put.rs`)
   - Accept `&Envelope` parameter
   - Serialize envelope to dCBOR bytes via `tagged_cbor().to_cbor_data()`
   - Validate serialized size (≤1000 bytes)
   - Derive signing key from ARID via HKDF
   - Create mutable item with seq=1, salt=None
   - Check if key already exists (get_mutable_most_recent)
   - Error if seq > 0 (AlreadyExists)
   - Publish mutable item with serialized envelope
   - Return PutReceipt with ARID, size, and metadata

4. **Get Implementation** (`mainline/get.rs`)
   - Derive signing key from ARID to compute target DHT key
   - Use get_mutable_most_recent (should have seq=1 if exists)
   - Polling loop with configurable timeout/interval
   - Retrieve bytes from DHT
   - Deserialize bytes to Envelope via `Envelope::try_from_cbor_data()`
   - Return envelope or None
   - Convert deserialization errors to GetError

## Design Philosophy

**Write-Once Semantics:**
- Each ARID key is written exactly once by the putter
- No support for updates, versioning, or multiple values per ARID
- Putter distributes ARID to getters via external means (out-of-band)
- Getters independently derive storage-layer keys from the ARID
- Simplified API with no CAS, sequence numbers, or conflict resolution

**Envelope-Based Values:**
- All values are Gordian Envelopes (`bc_envelope::Envelope`)
- Envelopes provide structured, extensible data format
- Native support for encryption, compression, signatures, and elision
- Deterministic dCBOR serialization for network transport
- Intrinsic Merkle digest tree for integrity verification
- Serialization: `envelope.tagged_cbor().to_cbor_data()` → bytes
- Deserialization: `Envelope::try_from_cbor_data(bytes)` → envelope

**Key Distribution Model:**
- Putter generates/chooses an ARID
- Putter creates an Envelope with data
- Putter writes envelope once to DHT/IPFS using ARID
- Putter shares ARID with getters (QR code, envelope, secure channel, etc.)
- Getters use same ARID to derive identical storage key and retrieve envelope
- ARID acts as lookup capability; application-layer encryption (if used) provides access control

## 1. BitTorrent Mainline DHT Key-Value Store

### 1.1 Architecture

#### Core Concepts

The BitTorrent mainline DHT (BEP-5/BEP-44) provides two storage modes:

1. **Immutable Storage** (BEP-44 immutable items)
   - Key: SHA-1 hash of the value (deterministic)
   - Value: Serialized Envelope as bytes (≤1 KiB after bencode encoding)
   - Immutable after storage
   - No authentication required

2. **Mutable Storage** (BEP-44 mutable items)
   - Key: Derived from ed25519 public key + optional salt
   - Value: Serialized Envelope as bytes (≤1 KiB after bencode encoding)
   - Updatable via sequence numbers (CAS semantics)
   - Signed with ed25519 private key
   - **Used for write-once with chosen keys** (updates not utilized)

#### Key Selection Strategy

For **write-once putter-chosen keys**, use mutable storage format without updates:

- **User-provided ARID** (32-byte identifier from `bc_components::ARID`)
- **HKDF-based key stretching** via `bc_crypto::hkdf_hmac_sha256`
- **Target DHT key** = SHA-1(pubkey || salt)
- **Write-once**: Always use seq=1, no subsequent updates
- **No DHT salt**: Single value per ARID (salt always None)

ARID-to-ed25519 derivation:
1. Input: `ARID` (32 bytes, from `bc_components::ARID`)
2. Salt: Context-specific constant (`b"hubert-mainline-dht-v1"`)
3. HKDF: `hkdf_hmac_sha256(arid.as_bytes(), salt, 32)` → ed25519 seed
4. SigningKey: `mainline::SigningKey::from_bytes(&seed)`
5. Publish with seq=1, salt=None (write-once)

This ensures:
- Deterministic key derivation from ARID
- Same ARID always resolves to same DHT location
- No complexity from updates or versioning
- Simplified put operation (no read-before-write)

#### API Design

```rust
use bc_components::ARID;
use bc_envelope::Envelope;

pub struct MainlineDhtKv {
    dht: AsyncDht,
    hkdf_salt: Vec<u8>,  // Context-specific salt for HKDF
}

pub struct PutOptions {
    /// Timeout for put operation
    pub timeout: Duration,
}

pub struct GetOptions {
    /// Poll until found or timeout
    pub poll_timeout: Duration,
    /// Interval between poll attempts
    pub poll_interval: Duration,
}

impl MainlineDhtKv {
    /// Create a new Mainline DHT KV store with custom HKDF salt
    pub fn new(dht: AsyncDht, hkdf_salt: impl AsRef<[u8]>) -> Self;

    /// Create with default HKDF salt ("hubert-mainline-dht-v1")
    pub fn with_default_salt(dht: AsyncDht) -> Self;

    /// Put envelope with ARID-based key (write-once)
    /// Serializes envelope to dCBOR and stores in DHT
    /// Returns error if key already exists (seq > 0)
    pub async fn put(
        &self,
        arid: &ARID,
        envelope: &Envelope,
        options: PutOptions,
    ) -> Result<PutReceipt, PutError>;

    /// Get envelope for ARID-based key
    /// Retrieves bytes from DHT and deserializes to Envelope
    pub async fn get(
        &self,
        arid: &ARID,
        options: GetOptions,
    ) -> Result<Option<Envelope>, GetError>;

    /// Check if ARID key exists (without fetching envelope)
    pub async fn exists(
        &self,
        arid: &ARID,
        options: GetOptions,
    ) -> Result<bool, GetError>;

    /// Derive ed25519 signing key from ARID (exposed for verification)
    pub fn derive_signing_key(&self, arid: &ARID) -> SigningKey;

    /// Get the public key for an ARID (for diagnostics)
    pub fn derive_public_key(&self, arid: &ARID) -> [u8; 32];
}

pub struct PutReceipt {
    pub target_id: Id,     // DHT lookup key
    pub pubkey: [u8; 32],  // Derived public key
    pub arid: ARID,        // Original ARID used
    pub envelope_size: usize, // Size of serialized envelope
}

#[derive(Debug, thiserror::Error)]
pub enum PutError {
    #[error("ARID already exists with sequence number {current_seq}")]
    AlreadyExists { current_seq: i64 },

    #[error("Envelope size {size} exceeds limit of {limit} bytes after bencode")]
    EnvelopeTooLarge { size: usize, limit: usize },

    #[error("DHT network error: {0}")]
    NetworkError(String),

    #[error("Operation timed out")]
    Timeout,

    #[error("Envelope serialization error: {0}")]
    EnvelopeError(#[from] bc_envelope::Error),

    #[error("CBOR error: {0}")]
    CborError(#[from] dcbor::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum GetError {
    #[error("DHT network error: {0}")]
    NetworkError(String),

    #[error("Operation timed out")]
    Timeout,

    #[error("Invalid ARID format")]
    InvalidArid,

    #[error("Envelope deserialization error: {0}")]
    EnvelopeError(#[from] bc_envelope::Error),

    #[error("CBOR error: {0}")]
    CborError(#[from] dcbor::Error),
}
```

#### Size Limits

- Envelope size: ≤1000 bytes after dCBOR serialization (conservative; BEP-44 limits bencode overhead)
- Envelopes can be compressed (`.compress()`) to fit within limits
- Envelopes can be elided (`.elide_revealing()`) to reduce size while preserving structure
- Total bencode representation must fit DHT constraints

### 1.2 Implementation Plan

#### Phase 1: Core Infrastructure

1. **ARID-to-Key Derivation Module** (`mainline/arid_derivation.rs`)
   - Import `bc_components::ARID` and `bc_crypto::hkdf_hmac_sha256`
   - Implement `derive_signing_key(arid: &ARID, hkdf_salt: &[u8]) -> SigningKey`
   - Implement `derive_public_key(arid: &ARID, hkdf_salt: &[u8]) -> [u8; 32]`
   - Default HKDF salt constant: `b"hubert-mainline-dht-v1"`
   - Unit tests for derivation determinism and consistency
   - Validate that same ARID always produces same signing key

2. **Value Encoding** (`mainline/encoding.rs`)
   - Envelope serialization: `envelope.tagged_cbor().to_cbor_data()`
   - Envelope deserialization: `Envelope::try_from_cbor_data(bytes)`
   - Validate envelope size after dCBOR serialization
   - Helper to estimate bencode overhead on top of dCBOR
   - Error types for size violations
   - Support for compressed envelopes (`.compress()` before put)

#### Phase 2: Basic Put/Get Operations

3. **Put Implementation** (`mainline/put.rs`)
   - Accept `&Envelope` parameter
   - Serialize envelope to dCBOR bytes via `tagged_cbor().to_cbor_data()`
   - Validate serialized size (≤1000 bytes)
   - Derive signing key from ARID via HKDF
   - Create mutable item with seq=1, salt=None
   - Check if key already exists (get_mutable_most_recent)
   - Error if seq > 0 (AlreadyExists)
   - Publish mutable item with serialized envelope
   - Return PutReceipt with ARID, size, and metadata

4. **Get Implementation** (`mainline/get.rs`)
   - Derive signing key from ARID to compute target DHT key
   - Use get_mutable_most_recent (should have seq=1 if exists)
   - Polling loop with configurable timeout/interval
   - Retrieve bytes from DHT
   - Deserialize bytes to Envelope via `Envelope::try_from_cbor_data()`
   - Return envelope or None
   - Convert deserialization errors to GetError

5. **Exists Check** (`mainline/get.rs`)
   - Lightweight check without fetching full value
   - Uses get_mutable_most_recent with minimal overhead
   - Returns bool

#### Phase 3: Error Handling & Validation

6. **Error Types** (`mainline/error.rs`)
   - Define error enums using `thiserror::Error` derive macro
   - `PutError` variants:
     - `AlreadyExists` - ARID already written (includes current seq)
     - `EnvelopeTooLarge` - Envelope size exceeds limit (includes actual and limit)
     - `NetworkError` - DHT communication failure
     - `Timeout` - Operation timed out
     - `EnvelopeError` - Envelope serialization/operations error
     - `CborError` - dCBOR encoding/decoding error
   - `GetError` variants:
     - `NetworkError` - DHT communication failure
     - `Timeout` - Operation timed out
     - `InvalidArid` - Malformed ARID input
     - `EnvelopeError` - Envelope deserialization error
     - `CborError` - dCBOR decoding error
   - All errors use `#[error("...")]` attribute for display messages
   - No `anyhow` in public API (only in tests via dev-dependencies)
   - Use `#[from]` attribute for automatic error conversions

7. **Envelope Validation**
   - Size checks after envelope serialization
   - ARID validation (proper 32-byte format)
   - Return structured errors with context
   - Suggest compression for oversized envelopes

#### Phase 4: Testing & Documentation

8. **Integration Tests**
   - Testnet roundtrips (fast, deterministic)
   - Mainnet roundtrips (ignored by default)
   - AlreadyExists error handling
   - ARID determinism tests

9. **Documentation**
   - API docs with examples
   - Write-once semantics clearly documented
   - Error handling patterns (using `Result<T, PutError>` and `Result<T, GetError>`)
   - Key distribution patterns
   - Security considerations (ARID as capability)

### 1.3 Error Handling Strategy

**Public API Errors:**
- All public API errors use `thiserror::Error` derive macro
- Structured error types: `PutError`, `GetError`
- Rich error context (e.g., sequence numbers, sizes, names)
- Display messages via `#[error("...")]` attributes
- No `anyhow::Error` in public signatures

**Test Code:**
- `anyhow` available as dev-dependency
- Tests use `anyhow::Result` for convenience
- Test utilities can use `.context()` for debugging
- Integration tests leverage `anyhow` for clarity

**Error Conversion:**
- Internal errors converted to public error types
- DHT errors → `NetworkError` variant
- Timeout detection → `Timeout` variant
- Validation failures → specific variants with context

### 1.4 Test Coverage Strategy

Based on existing tests:

- ✅ `mainline_immutable_roundtrip.rs` - Already validates immutable storage
- ✅ `mainline_mutable_roundtrip.rs` - Already validates mutable storage with selected keys
- **New tests needed:**
  - `mainline_kv_arid_basic.rs` - ARID-based write-once KV roundtrip
  - `mainline_kv_arid_determinism.rs` - Same ARID always derives same key
  - `mainline_kv_already_exists.rs` - Verify AlreadyExists error on duplicate put
  - `mainline_kv_size_limits.rs` - Boundary conditions
  - `mainline_kv_exists_check.rs` - Exists method validation

### 1.4 Security & Operational Considerations

**Privacy:**
- DHT operations and values are visible to network participants
- Envelopes not encrypted by default at storage layer
- Use envelope `.encrypt()` method for application-layer encryption
- ARID acts as lookup capability - anyone with ARID can retrieve envelope

**Authentication:**
- ed25519 signatures prevent unauthorized writes at DHT level
- Only holder of ARID can write to derived key location
- Use envelope `.add_signature()` for sender authentication if needed
- Write-once prevents tampering after publication

**Durability:**
- DHT nodes cache items temporarily (hours to days)
- No built-in re-publication (write-once design)
- Consider external re-publication service for long-term storage
- No persistence guarantees

**Performance:**
- Get latency: 100ms-5s depending on network
- Put replication delay: 1-5s typical
- Recommend polling with exponential backoff
- Single write per ARID (no update overhead)

**Key Distribution:**
- ARID must be shared out-of-band (QR, envelope, secure channel)
- ARID holder can retrieve envelope
- ARID acts as read capability (bearer token)
- No write capability distribution needed (write-once)

---

## 2. IPFS Key-Value Store

### 2.1 Architecture

#### Core Concepts

IPFS provides two storage modes relevant for KV operations:

1. **Immutable Storage** (Content-Addressed)
   - Key: CID (Content Identifier) - hash of serialized envelope
   - Value: Serialized Envelope as bytes (practical ~1-10 MB)
   - Immutable by definition
   - Automatic deduplication

2. **Mutable Storage** (IPNS - InterPlanetary Name System)
   - Key: IPNS name (derived from cryptographic keypair)
   - Value: Points to immutable CID (of serialized envelope)
   - Updatable via key holder
   - Built on libp2p pubsub + DHT records
   - **Used for write-once with chosen keys** (updates not utilized)

#### Key Selection Strategy

For **write-once putter-chosen keys**, use IPNS without updates:

- **User-provided ARID** (32-byte identifier from `bc_components::ARID`)
- **HKDF-based key stretching** via `bc_crypto::hkdf_hmac_sha256`
- **IPNS name** = Peer ID derived from ed25519 public key
- **Value indirection** = IPNS name → CID → serialized envelope bytes
- **Write-once**: Publish once, no subsequent updates

ARID-to-IPFS-keypair derivation:
1. Input: `ARID` (32 bytes, from `bc_components::ARID`)
2. Salt: Context-specific constant (`b"hubert-ipfs-ipns-v1"`)
3. HKDF: `hkdf_hmac_sha256(arid.as_bytes(), salt, 32)` → ed25519 seed
4. Keypair name: `format!("hubert-{}", arid.hex())`
5. Use IPFS `key_gen` API with derived name
6. Publish once, no re-publication

This ensures:
- Deterministic IPNS key generation from ARID
- Same ARID always resolves to same peer ID
- Simplified API (no update logic)
- No complexity from versioning or conflict resolution

#### API Design

```rust
use bc_components::ARID;
use bc_envelope::Envelope;

pub struct IpfsKv {
    client: IpfsClient,
    hkdf_salt: Vec<u8>,   // Context-specific salt for HKDF
    key_cache: KeyCache,  // ARID → (key_name, peer_id) mapping
}

pub struct PutOptions {
    /// TTL for IPNS record (None = daemon default, typically 24h)
    /// Note: No automatic re-publication in write-once design
    pub lifetime: Option<Duration>,
    /// Whether to pin the CID locally
    pub pin: bool,
}

pub struct GetOptions {
    /// Poll timeout for IPNS resolution
    pub resolve_timeout: Duration,
    /// Whether to use local cache or force DHT lookup
    pub nocache: bool,
}

impl IpfsKv {
    /// Create a new IPFS KV store with custom HKDF salt
    pub fn new(client: IpfsClient, hkdf_salt: impl AsRef<[u8]>) -> Self;

    /// Create with default HKDF salt ("hubert-ipfs-ipns-v1")
    pub fn with_default_salt(client: IpfsClient) -> Self;

    /// Put envelope with ARID-based key (write-once)
    /// Serializes envelope to dCBOR, adds to IPFS, publishes IPNS name
    /// Returns error if IPNS name already published
    pub async fn put(
        &self,
        arid: &ARID,
        envelope: &Envelope,
        options: PutOptions,
    ) -> Result<PutReceipt, PutError>;

    /// Get envelope for ARID-based key
    /// Resolves IPNS name → CID, retrieves bytes, deserializes to Envelope
    pub async fn get(
        &self,
        arid: &ARID,
        options: GetOptions,
    ) -> Result<Option<Envelope>, GetError>;

    /// Check if ARID key exists (without fetching envelope)
    pub async fn exists(
        &self,
        arid: &ARID,
        options: GetOptions,
    ) -> Result<bool, GetError>;

    /// List all managed ARID-based keys in local cache
    pub async fn list_keys(&self) -> Result<Vec<KeyInfo>, GetError>;

    /// Derive IPNS key name from ARID (exposed for verification)
    pub fn derive_key_name(&self, arid: &ARID) -> String;
}

pub struct PutReceipt {
    pub cid: String,       // The immutable CID stored
    pub ipns_name: String, // The IPNS name (peer ID)
    pub key_name: String,  // Daemon key name
    pub arid: ARID,        // Original ARID used
    pub envelope_size: usize, // Size of serialized envelope
}

pub struct KeyInfo {
    pub name: String,
    pub peer_id: String,
    pub arid: ARID,  // The ARID that generated this key
}

#[derive(Debug, thiserror::Error)]
pub enum PutError {
    #[error("IPNS name {ipns_name} already published")]
    AlreadyExists { ipns_name: String },

    #[error("Envelope size {size} exceeds practical limit")]
    EnvelopeTooLarge { size: usize },

    #[error("IPFS daemon error: {0}")]
    DaemonError(String),

    #[error("Operation timed out")]
    Timeout,

    #[error("Envelope serialization error: {0}")]
    EnvelopeError(#[from] bc_envelope::Error),

    #[error("CBOR error: {0}")]
    CborError(#[from] dcbor::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum GetError {
    #[error("IPFS daemon error: {0}")]
    DaemonError(String),

    #[error("IPNS resolution timed out")]
    Timeout,

    #[error("Invalid ARID format")]
    InvalidArid,

    #[error("Envelope deserialization error: {0}")]
    EnvelopeError(#[from] bc_envelope::Error),

    #[error("CBOR error: {0}")]
    CborError(#[from] dcbor::Error),
}
```

#### Size Limits

- Envelope size: Practical limit ~1-10 MB after dCBOR serialization
- No hard protocol limit, but larger envelopes may fail to propagate
- IPNS record size: Small (just points to CID)
- Envelopes can be compressed (`.compress()`) for efficiency
- Envelopes can be elided (`.elide_revealing()`) to reduce size### 2.2 Implementation Plan

#### Phase 1: Core Infrastructure

1. **ARID-to-Key Derivation Module** (`ipfs/arid_derivation.rs`)
   - Import `bc_components::ARID` and `bc_crypto::hkdf_hmac_sha256`
   - Implement `derive_key_name(arid: &ARID) -> String`
   - Key name format: `format!("hubert-{}", arid.hex())`
   - Default HKDF salt constant: `b"hubert-ipfs-ipns-v1"`
   - IPFS `key_gen` wrapper with collision detection
   - Unit tests for derivation determinism

2. **Key Cache** (`ipfs/key_cache.rs`)
   - In-memory cache: `ARID` → `(key_name, peer_id)`
   - Persistent cache option (serde JSON file)
   - Thread-safe access (Arc<RwLock<HashMap>>)
   - Cache invalidation on daemon restart detection

3. **Value Management** (`ipfs/value.rs`)
   - Envelope serialization: `envelope.tagged_cbor().to_cbor_data()`
   - Envelope deserialization: `Envelope::try_from_cbor_data(bytes)`
   - Add (upload) serialized envelope with size validation
   - Cat (download) bytes and deserialize to envelope
   - Pin/unpin helpers for CID management
   - Support for compressed envelopes

#### Phase 2: Basic Put/Get Operations

4. **Put Implementation** (`ipfs/put.rs`)
   - Accept `&Envelope` parameter
   - Derive/lookup IPNS key name from ARID
   - Check if IPNS name already exists (name_resolve)
   - Error if already published (AlreadyExists)
   - Serialize envelope to dCBOR bytes via `tagged_cbor().to_cbor_data()`
   - Add bytes to IPFS → get CID
   - Publish IPNS name → CID (once)
   - Optional pinning
   - Return PutReceipt with ARID, CID, and size

5. **Get Implementation** (`ipfs/get.rs`)
   - Derive/lookup IPNS key name from ARID
   - Resolve IPNS name → CID (with polling/retry)
   - Cat CID → bytes
   - Deserialize bytes to Envelope via `Envelope::try_from_cbor_data()`
   - Return envelope or None
   - Convert deserialization errors to GetError
   - Optional: cache resolved CID

6. **Exists Check** (`ipfs/get.rs`)
   - Check if IPNS name is published
   - Uses name_resolve without fetching value
   - Returns bool

#### Phase 3: Error Handling & Validation

7. **Error Types** (`ipfs/error.rs`)
   - Define error enums using `thiserror::Error` derive macro
   - `PutError` variants:
     - `AlreadyExists` - IPNS name already published (includes name)
     - `EnvelopeTooLarge` - Envelope size exceeds limit (includes size)
     - `DaemonError` - IPFS daemon issues
     - `Timeout` - Operation timed out
     - `EnvelopeError` - Envelope serialization/operations error
     - `CborError` - dCBOR encoding/decoding error
   - `GetError` variants:
     - `DaemonError` - IPFS daemon issues
     - `Timeout` - IPNS resolution timed out
     - `InvalidArid` - Malformed ARID input
     - `EnvelopeError` - Envelope deserialization error
     - `CborError` - dCBOR decoding error
   - All errors use `#[error("...")]` attribute for display messages
   - No `anyhow` in public API (only in tests via dev-dependencies)
   - Use `#[from]` attribute for automatic error conversions

8. **Envelope Validation**
   - Size checks after envelope serialization
   - ARID validation (proper 32-byte format)
   - Return structured errors with context
   - Suggest compression for large envelopes

#### Phase 4: Testing & Documentation

9. **Integration Tests**
   - Basic write-once roundtrip
   - ARID determinism tests
   - AlreadyExists error handling
   - Large value tests (>1 MB)
   - Pin lifecycle tests

10. **Documentation**
    - API docs with examples
    - Write-once semantics clearly documented
    - Error handling patterns (using `Result<T, PutError>` and `Result<T, GetError>`)
    - Key distribution patterns
    - Daemon configuration requirements

### 2.3 Error Handling Strategy

**Public API Errors:**
- All public API errors use `thiserror::Error` derive macro
- Structured error types: `PutError`, `GetError`
- Rich error context (e.g., IPNS names, sizes)
- Display messages via `#[error("...")]` attributes
- No `anyhow::Error` in public signatures

**Test Code:**
- `anyhow` available as dev-dependency
- Tests use `anyhow::Result` for convenience
- Test utilities can use `.context()` for debugging
- Integration tests leverage `anyhow` for clarity

**Error Conversion:**
- IPFS API errors → `DaemonError` variant
- Timeout detection → `Timeout` variant
- Validation failures → specific variants with context
- Preserve error chains where helpful

### 2.4 Test Coverage Strategy

Based on existing tests:

- ✅ `ipfs_immutable_roundtrip.rs` - Validates basic add/cat
- ✅ `ipfs_mutable_roundtrip.rs` - Validates IPNS publish/resolve with key_gen
- **New tests needed:**
  - `ipfs_kv_arid_basic.rs` - ARID-based write-once KV roundtrip
  - `ipfs_kv_arid_determinism.rs` - Same ARID always derives same IPNS key
  - `ipfs_kv_already_exists.rs` - Verify AlreadyExists error on duplicate put
  - `ipfs_kv_cache.rs` - Key cache behavior
  - `ipfs_kv_large_values.rs` - Multi-MB values
  - `ipfs_kv_exists_check.rs` - Exists method validation

### 2.4 Security & Operational Considerations

**Privacy:**
- Content is public and discoverable by CID
- IPNS names are public (peer IDs)
- DHT lookups are visible to network
- Envelopes not encrypted by default at storage layer
- Use envelope `.encrypt()` method for application-layer encryption
- ARID acts as lookup capability - anyone with ARID can retrieve envelope

**Authentication:**
- IPNS updates authenticated via ed25519 signatures
- Only holder of ARID can write to derived IPNS name
- Use envelope `.add_signature()` for sender authentication if needed
- CID immutability provides integrity
- Write-once prevents tampering after publication

**Durability:**
- Requires pinning for persistence
- Unpinned content subject to garbage collection
- IPNS records expire (default 24h)
- No automatic re-publication in write-once design
- Consider external pinning service for long-term storage

**Performance:**
- Add latency: 10ms-1s (local daemon)
- IPNS resolve: 1-10s (DHT propagation)
- Cat latency: 10ms-5s (depending on providers)
- Single publish per ARID (no update overhead)

**Dependencies:**
- Requires running Kubo daemon (or compatible IPFS node)
- Default RPC endpoint: http://127.0.0.1:5001
- Network connectivity required for DHT operations

**Key Distribution:**
- ARID must be shared out-of-band (QR, envelope, secure channel)
- ARID holder can retrieve envelope
- ARID acts as read capability (bearer token)
- No write capability distribution needed (write-once)

---

## 3. Hybrid Storage Layer (DHT with IPFS Fallback)

**Status**: Not yet implemented. This section describes the planned architecture.

### 3.1 Architecture

The Hybrid storage layer combines DHT and IPFS to optimize for both speed and capacity:

**Strategy:**
- **Small envelopes (≤1000 bytes)**: Store directly in DHT
- **Large envelopes (>1000 bytes)**: Store reference envelope in DHT, actual envelope in IPFS
- **Transparent to caller**: API handles indirection automatically

**Reference Envelope Format:**
```
'' [                            // Unit subject (empty)
    'derefernceVia': "ipfs",    // Known value indicating indirection
    'id': <ARID>,               // New ARID for IPFS lookup
    'size': <usize>             // Size of actual envelope (for diagnostics)
]
```

### 3.2 Put Operation Flow

```
1. Serialize target envelope to dCBOR
2. Check serialized size
3. IF size ≤ DHT_LIMIT (1000 bytes):
     - Store envelope directly in DHT using original ARID
     - Return receipt with DHT-only indicator
   ELSE:
     - Generate new ARID (reference_arid) for IPFS storage
     - Create reference envelope with reference_arid
     - Encrypt reference envelope using key derived from original ARID
     - Store encrypted reference envelope in DHT using original ARID
     - Store actual envelope in IPFS using reference_arid
     - Return receipt with hybrid indicator (DHT + IPFS)
```

### 3.3 Get Operation Flow

```
1. Retrieve envelope from DHT using ARID
2. Check if envelope is reference envelope by attempting to decrypt using key derived from ARID.
3. If decryption fails:
     - Treat as normal DHT envelope
4. If decryption succeeds:
     - Validate reference envelope structure including `'derefernceVia': "ipfs"`, `'id': <ARID>`, and `size: <usize>`
     - Extract reference_arid from assertions
     - Retrieve actual envelope from IPFS using reference_arid
     - Return actual envelope
```

### 3.4 API Design

Provisional and subject to refinement:

```rust
use bc_components::ARID;
use bc_envelope::Envelope;

pub struct HybridKv {
    dht: MainlineDhtKv,
    ipfs: IpfsKv,
    dht_size_limit: usize,  // Default: 1000 bytes
}

pub struct PutOptions {
    /// Timeout for put operation
    pub timeout: Duration,
    /// Force IPFS storage even for small envelopes
    pub force_ipfs: bool,
    /// Whether to pin IPFS content
    pub pin: bool,
}

pub struct GetOptions {
    /// Poll timeout for DHT
    pub dht_timeout: Duration,
    /// Poll timeout for IPFS resolution (if needed)
    pub ipfs_timeout: Duration,
    /// Interval between poll attempts
    pub poll_interval: Duration,
}

impl HybridKv {
    /// Create a new Hybrid KV store
    pub fn new(dht: MainlineDhtKv, ipfs: IpfsKv) -> Self;

    /// Create with custom DHT size limit
    pub fn with_size_limit(
        dht: MainlineDhtKv,
        ipfs: IpfsKv,
        dht_size_limit: usize,
    ) -> Self;

    /// Put envelope with ARID-based key (write-once)
    /// Automatically uses DHT or DHT+IPFS based on size
    pub async fn put(
        &self,
        arid: &ARID,
        envelope: &Envelope,
        options: PutOptions,
    ) -> Result<PutReceipt, PutError>;

    /// Get envelope for ARID-based key
    /// Automatically handles DHT-only or DHT+IPFS indirection
    pub async fn get(
        &self,
        arid: &ARID,
        options: GetOptions,
    ) -> Result<Option<Envelope>, GetError>;

    /// Check if ARID key exists (checks DHT only)
    pub async fn exists(
        &self,
        arid: &ARID,
        options: GetOptions,
    ) -> Result<bool, GetError>;

    /// Check storage location for diagnostic purposes
    pub async fn storage_info(
        &self,
        arid: &ARID,
    ) -> Result<Option<StorageInfo>, GetError>;
}

pub struct PutReceipt {
    pub arid: ARID,
    pub storage: StorageLocation,
    pub envelope_size: usize,
}

pub enum StorageLocation {
    /// Stored directly in DHT
    DhtOnly {
        target_id: mainline::Id,
    },
    /// Stored as reference in DHT, actual data in IPFS
    Hybrid {
        dht_target_id: mainline::Id,
        reference_arid: ARID,
        ipfs_cid: String,
        ipfs_name: String,
    },
}

pub struct StorageInfo {
    pub location: StorageLocation,
    pub is_reference: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum PutError {
    #[error("ARID already exists")]
    AlreadyExists,

    #[error("DHT error: {0}")]
    DhtError(#[from] super::mainline::PutError),

    #[error("IPFS error: {0}")]
    IpfsError(#[from] super::ipfs::PutError),

    #[error("Reference envelope creation failed: {0}")]
    ReferenceCreationError(String),
}

#[derive(Debug, thiserror::Error)]
pub enum GetError {
    #[error("DHT error: {0}")]
    DhtError(#[from] super::mainline::GetError),

    #[error("IPFS error: {0}")]
    IpfsError(#[from] super::ipfs::GetError),

    #[error("Invalid reference envelope: {0}")]
    InvalidReference(String),

    #[error("Reference ARID not found in IPFS")]
    ReferenceNotFound,
}
```

### 3.5 Reference Envelope Details

Make sure you understand the following before implementing:
- The `queries.rs` file in `bc_envelope` for envelope manipulation
- The `encrypt()` and `decrypt()` methods in `encrypt.rs` in `bc_envelope` for wrapping/encrypting and decrypting/unwrapping envelopes in one step.
- `SymmetricKey` in `bc_components`.
- `hkdf_hmac_sha256` in `bc_crypto` for deriving keys.

**Plain Reference Envelope (Basic API):**
```rust
// Creating reference envelope
fn create_reference_envelope(
    reference_arid: &ARID,
    actual_size: usize,
) -> Envelope {
    Envelope::unit()
        .add_assertion(known_values::DEREFERENCE_VIA, "ipfs")
        .add_assertion(known_values::ID, reference_arid)
        .add_assertion("size", actual_size)
}
```

### 3.7 Implementation Plan

#### Phase 1: Reference Envelope Infrastructure

1. **Reference Envelope Module** (`hybrid/reference.rs`)
   - Define reference envelope format
   - Create reference envelope builder (plain)
   - Parse and validate reference envelopes
   - Extract reference ARID from reference envelope
   - Detect if envelope is reference
   - Unit tests for reference envelope operations

2. **Size Estimation** (`hybrid/sizing.rs`)
   - Accurate dCBOR size calculation
   - Bencode overhead estimation for DHT
   - Helper to determine if envelope fits in DHT
   - Include reference envelope overhead in calculations

#### Phase 2: Hybrid Put/Get Operations

3. **Put Implementation** (`hybrid/put.rs`)
   - Serialize and measure envelope size
   - Decision logic: DHT-only vs Hybrid
   - **DHT-only path**: Direct storage in DHT
   - **Hybrid path**:
     - Generate new ARID for IPFS
     - Create plain reference envelope
     - Store actual envelope in IPFS
     - Store reference in DHT
   - Return appropriate PutReceipt with location info
   - Handle AlreadyExists from both layers

4. **Get Implementation** (`hybrid/get.rs`)
   - Retrieve envelope from DHT
   - Check if reference envelope (plain format)
   - **Direct path**: Return DHT envelope
   - **Hybrid path**:
     - Extract reference ARID
     - Retrieve from IPFS using reference ARID
     - Return actual envelope
   - Handle missing references gracefully

5. **Exists Check** (`hybrid/exists.rs`)
   - Check DHT only (references count as existing)
   - Optional: verify IPFS reference is retrievable

#### Phase 3: Error Handling & Validation

8. **Error Types** (`hybrid/error.rs`)
   - `PutError` with DHT and IPFS variants
   - `GetError` with reference-specific errors
   - InvalidReference for malformed reference envelopes
   - ReferenceNotFound for missing IPFS content
   - Proper error conversion from underlying layers

9. **Storage Info** (`hybrid/info.rs`)
   - Diagnostic API to check storage location
   - Return whether envelope is reference or direct
   - Include size and location metadata

#### Phase 4: Testing & Documentation

10. **Integration Tests**
    - Small envelope roundtrip (DHT-only path)
    - Large envelope roundtrip (Hybrid path)
    - Missing reference handling
    - AlreadyExists with both DHT and IPFS
    - Size boundary conditions (exactly at limit)

11. **Documentation**
    - Hybrid storage strategy explained
    - When DHT vs IPFS is used
    - Reference envelope format specification
    - Performance characteristics
    - Troubleshooting missing references

### 3.8 Size Limits & Tradeoffs

**DHT Size Limit:**
- Conservative: 1000 bytes serialized dCBOR
- Plain reference envelope overhead: ~100 bytes
- Effective payload for hybrid: ~900 bytes

**Decision Points:**
- Envelope < 1000 bytes → DHT-only (optimal: fast, no dependencies)
- Envelope ≥ 1000 bytes → Hybrid (DHT reference + IPFS storage)

**Tradeoffs:**
| Aspect        | DHT-Only                 | Hybrid (DHT + IPFS)           |
| ------------- | ------------------------ | ----------------------------- |
| Latency       | 100ms-5s (single lookup) | 100ms-5s (DHT) + 1-10s (IPFS) |
| Size limit    | 1 KB                     | ~10 MB (practical)            |
| Dependencies  | None                     | Kubo daemon                   |
| Durability    | Hours-days               | Requires pinning              |
| Complexity    | Simple                   | Two-layer indirection         |
| Failure modes | DHT unavailable          | DHT or IPFS unavailable       |

### 3.8 Operational Considerations

**Consistency:**
- Reference envelope and IPFS content written atomically (from caller perspective)
- If IPFS write fails, DHT write is not attempted
- AlreadyExists checked on both layers

**Garbage Collection:**
- IPFS content requires pinning
- Unpinned IPFS references become dangling
- No automatic cleanup of IPFS content when DHT expires

**Failure Recovery:**
- Missing IPFS reference returns `ReferenceNotFound` error
- Caller can retry put operation with new ARID
- Consider external monitoring for dangling references

---

## 4. Unified API Considerations

### 4.1 Common Traits

Both implementations use ARID-based write-once keys with Envelope values and implement shared traits:

```rust
use bc_components::ARID;
use bc_envelope::Envelope;

pub trait KvStore: Send + Sync {
    /// Put envelope with ARID key (write-once, errors if exists)
    async fn put(&self, arid: &ARID, envelope: &Envelope) -> Result<String, Box<dyn std::error::Error>>;
    /// Get envelope for ARID key
    async fn get(&self, arid: &ARID) -> Result<Option<Envelope>, Box<dyn std::error::Error>>;
    /// Check if ARID key exists
    async fn exists(&self, arid: &ARID) -> Result<bool, Box<dyn std::error::Error>>;
}

// Mainline, IPFS, and Hybrid each implement this trait
impl KvStore for MainlineDhtKv { /* ... */ }
impl KvStore for IpfsKv { /* ... */ }
impl KvStore for HybridKv { /* ... */ }
```

### 4.2 Abstraction Layers

Potential unified interface:

```rust
pub enum BackendType {
    MainlineDht,
    Ipfs,
}

pub struct HubertKv {
    backend: Box<dyn KvStore>,
}

impl HubertKv {
    pub fn new(backend_type: BackendType, config: Config) -> Result<Self>;
    // Delegates to backend implementation
}
```

### 4.3 Tradeoffs Summary

| Feature         | Mainline DHT           | IPFS                               | Hybrid (DHT + IPFS)              |
| --------------- | ---------------------- | ---------------------------------- | -------------------------------- |
| Max value size  | ~1 KB                  | ~1-10 MB (practical)               | ~1-10 MB (automatic fallback)    |
| Get latency     | 100ms-5s               | 1-10s (IPNS), 10ms-1s (immutable)  | 100ms-5s (small), 1-15s (large)  |
| Put latency     | 1-5s                   | 10ms-1s (add), 1-5s (IPNS publish) | 1-5s (small), 2-10s (large)      |
| Write semantics | Write-once (seq=1)     | Write-once (IPNS publish once)     | Write-once (both layers)         |
| Durability      | Temporary (hours-days) | Requires pinning                   | DHT temporary, IPFS pinned       |
| Dependencies    | None (pure DHT)        | Kubo daemon                        | Kubo daemon (for large values)   |
| Network usage   | UDP                    | TCP/QUIC/WebSocket                 | UDP + TCP/QUIC (when needed)     |
| Privacy         | Public                 | Public                             | Public                           |
| Auth model      | ed25519 signatures     | ed25519 signatures (IPNS)          | ed25519 (both layers)            |
| ARID capability | Read-only (via ARID)   | Read-only (via ARID)               | Read-only (via ARID)             |
| Complexity      | Simple                 | Moderate                           | Moderate (two-layer indirection) |
| Failure modes   | DHT unavailable        | IPFS daemon down                   | DHT or IPFS (for large) down     |
| **Best for**    | Small, fast lookups    | Large payloads, persistence        | Automatic optimization by size   |

---

## 5. Implementation Sequencing

Recommended order:

1. **Mainline DHT KV** (simpler, fewer dependencies)
   - Leverage existing test patterns
   - Build ARID derivation module
   - Build envelope serialization module
   - Implement basic put/get with envelopes
   - Add write-once enforcement (AlreadyExists errors)
   - Document and test

2. **IPFS KV** (more complex, daemon-dependent)
   - Leverage existing IPNS test patterns
   - Build ARID derivation module
   - Build envelope serialization module
   - Implement IPNS-based put/get with envelopes
   - Add write-once enforcement
   - Build key cache system
   - Add pinning lifecycle
   - Document and test

3. **Hybrid KV** (combines DHT and IPFS)
   - Build reference envelope infrastructure (plain)
   - Implement size-based routing logic
   - Implement put operation (DHT or DHT+IPFS)
   - Implement get operation with indirection handling
   - Add storage info diagnostics
   - Document and test

4. **Unified Interface** (optional)
   - Define common traits
   - Implement backend abstraction
   - Add backend-specific configuration
   - Integration tests across all backends

---

## 6. Testing Requirements

### 6.1 Mainline DHT Tests

- [ ] Basic ARID-based write-once put/get roundtrip (testnet)
- [ ] Basic ARID-based write-once put/get roundtrip (mainnet, ignored)
- [ ] ARID derivation determinism (same ARID → same signing key)
- [ ] HKDF salt variation (different salts → different keys)
- [ ] AlreadyExists error on duplicate put attempt
- [ ] Envelope size limit enforcement
- [ ] Exists check (without fetching envelope)
- [ ] Polling timeout behavior
- [ ] Network partition recovery

### 6.2 IPFS Tests

- [ ] Basic ARID-based write-once put/get roundtrip (immutable)
- [ ] Basic ARID-based write-once put/get roundtrip (IPNS mutable)
- [ ] ARID derivation determinism (same ARID → same IPNS key)
- [ ] HKDF salt variation (different salts → different peer IDs)
- [ ] AlreadyExists error on duplicate IPNS publish
- [ ] Key cache hit/miss
- [ ] Large envelope handling (>1 MB)
- [ ] Pin lifecycle (add once, verify pinned)
- [ ] Exists check (without fetching envelope)
- [ ] Daemon connection failure handling
- [ ] IPNS resolution timeout

### 6.3 Hybrid Tests

- [ ] Small envelope roundtrip (DHT-only path, <1000 bytes)
- [ ] Large envelope roundtrip (Hybrid path, >1000 bytes)
- [ ] Boundary condition (exactly 1000 bytes)
- [ ] Reference envelope creation and parsing (plain)
- [ ] Reference ARID extraction
- [ ] Missing IPFS reference error handling
- [ ] AlreadyExists on DHT layer
- [ ] AlreadyExists on IPFS layer (for large envelopes)
- [ ] Storage info API (check DHT vs Hybrid)
- [ ] Force IPFS option (bypass DHT for small envelopes)

### 6.4 Integration Tests

- [ ] Backend switching (same ARID interface)
- [ ] Performance benchmarks (latency, throughput) across all backends
- [ ] Concurrent readers (same ARID, multiple getters)
- [ ] Error handling consistency across backends

---

## 7. Documentation Deliverables

For each API (DHT, IPFS, Hybrid):

1. **API Reference** - Rustdoc with examples
2. **User Guide** - Setup, usage patterns, best practices
3. **Security Guide** - Threat model, key management, encryption
4. **Performance Guide** - Tuning, caching, size optimization
5. **Migration Guide** - Switching backends, data portability
6. **Hybrid Guide** - When to use Hybrid, reference envelope format, troubleshooting

---

## 8. ARID Key Management

### 8.1 ARID Properties

From `bc_components::ARID`:
- Fixed 32-byte (256-bit) cryptographically strong identifier
- Statistically random, non-correlatable bits
- Neutral semantics (no inherent type information)
- Suitable as input to cryptographic constructs (like HKDF)
- Stable identifiers for mutable data structures

### 8.2 HKDF Derivation Details

Both backends use HKDF-HMAC-SHA-256 from `bc_crypto`:

```rust
use bc_components::ARID;
use bc_crypto::hkdf_hmac_sha256;

// Mainline DHT derivation
fn derive_mainline_signing_key(arid: &ARID) -> [u8; 32] {
    let salt = b"hubert-mainline-dht-v1";
    let seed = hkdf_hmac_sha256(arid.as_bytes(), salt, 32);
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&seed);
    arr
}

// IPFS IPNS derivation (for key name)
fn derive_ipfs_key_name(arid: &ARID) -> String {
    format!("hubert-{}", arid.hex())
}

// Hybrid uses same derivation as DHT for primary ARID
// and generates new ARID for IPFS reference when needed
```

### 8.3 Salt Management

**HKDF Salt Purpose:**
- Domain separation between different applications/versions
- Prevents cross-protocol key reuse
- Default salts are versioned for protocol evolution

**Default Salts:**
- Mainline DHT: `b"hubert-mainline-dht-v1"`
- IPFS IPNS: `b"hubert-ipfs-ipns-v1"`
- Hybrid: Uses DHT salt for primary, IPFS salt for references

**No DHT Salt (Mainline):**
- BEP-44 supports optional salt for multiple values per pubkey
- Write-once design always uses salt=None
- Single value per ARID simplifies design
- No namespace management needed

### 8.4 Key Derivation Guarantees

1. **Determinism**: Same ARID + same HKDF salt → same derived key (always)
2. **Isolation**: Different HKDF salts → statistically independent keys
3. **Irreversibility**: Cannot derive ARID from public key or DHT key
4. **Collision Resistance**: Cryptographic strength inherited from HKDF-HMAC-SHA-256
5. **Cross-Network Isolation**: Mainline and IPFS keys are independent (different salts)
6. **Reference ARID Independence**: Hybrid reference ARIDs are independent from primary ARID

### 8.5 Security Considerations

**ARID Storage:**
- Users must securely store their ARIDs to retain write capability
- Loss of ARID = loss of ability to prove authorship
- Getters only need ARID for read access (ARID as capability)
- Consider using `bc-envelope` for encrypted ARID storage

**Write-Once Guarantees:**
- Once written, value cannot be updated by anyone (including original putter)
- AlreadyExists errors prevent accidental overwrites
- Integrity protected by cryptographic signatures
- No CAS complexity or race conditions

**Network Visibility:**
- Public keys are visible on DHT/IPFS networks
- ARID itself is NOT published (only derived public keys)
- Envelope bytes are visible to network (opaque binary data)
- Unencrypted envelopes visible to network
- Use envelope `.encrypt()` for confidentiality at application layer
- IPNS names and CIDs are discoverable

**Key Distribution:**
- ARID distribution is out-of-band (application responsibility)
- ARID acts as read capability (bearer token)
- Secure channel recommended for ARID sharing (Signal, envelope, QR)
- No need to distribute signing keys (write-once)

---

## Notes for AI Agents

When implementing:

- **Write-once semantics**: No support for updates, CAS, or multiple values per ARID
- **Envelope-based values**: All values MUST be `bc_envelope::Envelope` instances
- All keys MUST be `bc_components::ARID` instances (32 bytes)
- Use `bc_crypto::hkdf_hmac_sha256` for all key derivation
- Respect HKDF salt constants for domain separation
- Mainline: Always use seq=1, salt=None for mutable items
- IPFS: Publish IPNS name once, no re-publication
- Error on duplicate put attempts (AlreadyExists)

**Envelope Operations:**
- **Serialization**: `envelope.tagged_cbor().to_cbor_data()` → Vec<u8>
- **Deserialization**: `Envelope::try_from_cbor_data(bytes)` → Result<Envelope>
- Envelopes use deterministic dCBOR encoding (via `dcbor` crate)
- Envelopes support compression: `envelope.compress()` for size reduction
- Envelopes support elision: `envelope.elide_revealing(...)` for selective disclosure
- Envelopes have intrinsic digest tree for integrity
- Review `bc_envelope` API, especially `queries.rs` for data extraction
- Use `Envelope::new()`, `.add_assertion()`, `.subject()`, etc.

**Error Handling:**
- **Public API**: Use `thiserror::Error` derive macro for all error types
- **Tests only**: Use `anyhow::Result` (dev-dependency only)
- Define structured error enums (`PutError`, `GetError`)
- Include `EnvelopeError` and `CborError` variants with `#[from]` attribute
- Include context in error variants (seq numbers, sizes, names)
- Use `#[error("...")]` attributes for display messages
- Never expose `anyhow::Error` in public API signatures
- Convert internal errors to structured public error types

**Implementation Guidelines:**
- Start with immutable storage tests as foundation
- Build write-once mutable storage on top (both DHT and IPNS)
- Validate ARID input before any network operations
- Serialize envelope and validate size before network operations
- Implement polling with exponential backoff for gets
- Handle envelope serialization/deserialization errors properly
- Provide clear error messages via thiserror error strings
- Follow existing test patterns (testnet + ignored mainnet)
- Document write-once semantics prominently in all API docs
- Document envelope serialization format in API docs
- Document error types with examples in rustdoc
- Document ARID-to-key derivation in module docs

**Build & Quality:**
- Run `cargo +nightly fmt` only in `hubert/` crate after edits
- Run `cargo clippy` in `hubert/` before ending turn
- Do not stage or commit without direction
