# Hubert: Distributed Key-Value Store APIs

This document outlines architectures and implementation plans for using BitTorrent mainline DHT and IPFS as key-value stores where putters choose their own keys.

## 1. BitTorrent Mainline DHT Key-Value Store

### 1.1 Architecture

#### Core Concepts

The BitTorrent mainline DHT (BEP-5/BEP-44) provides two storage modes:

1. **Immutable Storage** (BEP-44 immutable items)
   - Key: SHA-1 hash of the value (deterministic)
   - Value: Arbitrary bytes (≤1 KiB after bencode encoding)
   - Immutable after storage
   - No authentication required

2. **Mutable Storage** (BEP-44 mutable items)
   - Key: Derived from ed25519 public key + optional salt
   - Value: Arbitrary bytes (≤1 KiB after bencode encoding)
   - Updatable via sequence numbers (CAS semantics)
   - Signed with ed25519 private key
   - Supports versioning and concurrent updates

#### Key Selection Strategy

For **putter-chosen keys**, use mutable storage:

- **User-provided key material** → Derive ed25519 signing key
- **Optional salt** → Allows multiple values under same public key
- **Target DHT key** = SHA-1(pubkey || salt)
- Putter retains signing key for updates

Key derivation options:
1. Hash user's string key to 32-byte seed → SigningKey
2. Use KDF (HKDF-SHA256) on user key + context → SigningKey
3. Direct 32-byte key input (advanced users)

#### API Design

```rust
pub struct MainlineDhtKv {
    dht: AsyncDht,
    key_derivation: KeyDerivationMode,
}

pub enum KeyDerivationMode {
    /// SHA-256 hash of user key → ed25519 seed
    HashToSeed,
    /// HKDF-SHA256(user_key, salt, info) → ed25519 seed
    Hkdf { info: Vec<u8> },
}

pub struct PutOptions {
    /// Optional salt for key isolation (max 64 bytes recommended)
    pub salt: Option<Vec<u8>>,
    /// CAS: only update if current seq matches
    pub expected_seq: Option<i64>,
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
    /// Put value with user-chosen key
    pub async fn put(
        &self,
        user_key: &[u8],
        value: &[u8],
        options: PutOptions,
    ) -> Result<PutReceipt>;

    /// Get most recent value for user-chosen key
    pub async fn get(
        &self,
        user_key: &[u8],
        salt: Option<&[u8]>,
        options: GetOptions,
    ) -> Result<Option<Vec<u8>>>;

    /// Get with metadata (sequence number, timestamp)
    pub async fn get_with_meta(
        &self,
        user_key: &[u8],
        salt: Option<&[u8]>,
        options: GetOptions,
    ) -> Result<Option<MutableItemResponse>>;

    /// Delete by publishing empty value (semantic convention)
    pub async fn delete(
        &self,
        user_key: &[u8],
        salt: Option<&[u8]>,
    ) -> Result<()>;
}

pub struct PutReceipt {
    pub target_id: Id,  // DHT lookup key
    pub seq: i64,       // Sequence number written
    pub pubkey: [u8; 32],
}

pub struct MutableItemResponse {
    pub value: Vec<u8>,
    pub seq: i64,
    pub key: [u8; 32],
    pub timestamp: SystemTime,
}
```

#### Size Limits

- Value size: ≤1000 bytes (conservative limit; BEP-44 limits bencode overhead)
- Salt size: ≤64 bytes recommended
- Total bencode representation must fit DHT constraints

### 1.2 Implementation Plan

#### Phase 1: Core Infrastructure

1. **Key Derivation Module** (`mainline/key_derivation.rs`)
   - Implement SHA-256 hash-to-seed
   - Implement HKDF-based derivation
   - Validate user key inputs
   - Unit tests for derivation consistency

2. **Signing Key Management** (`mainline/signing.rs`)
   - Wrapper around mainline's SigningKey
   - Cache derived keys (optional, with security considerations)
   - Key serialization/deserialization helpers

3. **Value Encoding** (`mainline/encoding.rs`)
   - Validate value size before bencode
   - Helper to estimate bencode overhead
   - Error types for size violations

#### Phase 2: Basic Put/Get Operations

4. **Put Implementation** (`mainline/put.rs`)
   - Derive signing key from user key
   - Read-most-recent → compute next seq → CAS
   - Retry logic for CAS failures
   - Return PutReceipt with metadata

5. **Get Implementation** (`mainline/get.rs`)
   - Derive target DHT key
   - Polling loop with configurable timeout/interval
   - Return raw value or structured response

#### Phase 3: Advanced Features

6. **Conflict Resolution**
   - CAS retry with exponential backoff
   - Optional: last-write-wins mode
   - Optional: custom merge strategies

7. **Caching Layer**
   - Local cache of recent gets
   - TTL-based invalidation
   - Option to skip cache (nocache flag)

8. **Batch Operations**
   - Multi-get (parallel lookups)
   - Multi-put (sequential to preserve ordering)

#### Phase 4: Testing & Refinement

9. **Integration Tests**
   - Testnet roundtrips (fast, deterministic)
   - Mainnet roundtrips (ignored by default)
   - Concurrent put tests (CAS validation)
   - Salt collision tests

10. **Documentation**
    - API docs with examples
    - Security considerations (key management, DHT visibility)
    - Performance tuning guide

### 1.3 Test Coverage Strategy

Based on existing tests:

- ✅ `mainline_immutable_roundtrip.rs` - Already validates immutable storage
- ✅ `mainline_mutable_roundtrip.rs` - Already validates mutable storage with selected keys
- **New tests needed:**
  - `mainline_kv_basic.rs` - High-level API roundtrip
  - `mainline_kv_cas.rs` - Concurrent update scenarios
  - `mainline_kv_salt.rs` - Multiple values under one key
  - `mainline_kv_size_limits.rs` - Boundary conditions

### 1.4 Security & Operational Considerations

**Privacy:**
- All DHT operations are public (visible to network participants)
- Keys and values are not encrypted by default
- Consider application-layer encryption for sensitive data

**Authentication:**
- ed25519 signatures prevent unauthorized updates
- Only holder of signing key can update values
- Salt provides namespace isolation per application

**Durability:**
- DHT nodes cache items temporarily (hours to days)
- Re-publication required for long-term storage
- No persistence guarantees

**Performance:**
- Get latency: 100ms-5s depending on network
- Put replication delay: 1-5s typical
- Recommend polling with exponential backoff

---

## 2. IPFS Key-Value Store

### 2.1 Architecture

#### Core Concepts

IPFS provides two storage modes relevant for KV operations:

1. **Immutable Storage** (Content-Addressed)
   - Key: CID (Content Identifier) - hash of content
   - Value: Arbitrary bytes (no hard limit, but practical ~1-10 MB)
   - Immutable by definition
   - Automatic deduplication

2. **Mutable Storage** (IPNS - InterPlanetary Name System)
   - Key: IPNS name (derived from cryptographic keypair)
   - Value: Points to immutable CID
   - Updatable via key holder
   - Built on libp2p pubsub + DHT records

#### Key Selection Strategy

For **putter-chosen keys**, use IPNS with deterministic key generation:

- **User-provided key material** → Generate IPNS ed25519 keypair
- **IPNS name** = Peer ID derived from public key
- **Value indirection** = IPNS name → CID → actual bytes
- Putter retains IPNS private key for updates

Key derivation options:
1. Hash user's string key to keypair seed
2. Use KDF on user key + context
3. Named keys managed by IPFS daemon (key_gen API)

#### API Design

```rust
pub struct IpfsKv {
    client: IpfsClient,
    key_derivation: IpnsKeyDerivationMode,
    key_cache: KeyCache,  // Avoid redundant key_gen calls
}

pub enum IpnsKeyDerivationMode {
    /// Hash user key to keypair name (stored in IPFS daemon)
    NamedKeys,
    /// External keypair management (not daemon-stored)
    External,
}

pub struct PutOptions {
    /// TTL for IPNS record (None = daemon default, typically 24h)
    pub lifetime: Option<Duration>,
    /// Whether to resolve input path (typically false for CID)
    pub resolve: bool,
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
    /// Put value with user-chosen key
    pub async fn put(
        &self,
        user_key: &str,
        value: &[u8],
        options: PutOptions,
    ) -> Result<PutReceipt>;

    /// Get current value for user-chosen key
    pub async fn get(
        &self,
        user_key: &str,
        options: GetOptions,
    ) -> Result<Option<Vec<u8>>>;

    /// Get with metadata (CID, timestamp)
    pub async fn get_with_meta(
        &self,
        user_key: &str,
        options: GetOptions,
    ) -> Result<Option<IpnsResponse>>;

    /// Unpin and optionally unpublish IPNS name
    pub async fn delete(
        &self,
        user_key: &str,
        unpin_cids: bool,
    ) -> Result<()>;

    /// List all managed keys
    pub async fn list_keys(&self) -> Result<Vec<KeyInfo>>;
}

pub struct PutReceipt {
    pub cid: String,      // The immutable CID stored
    pub ipns_name: String, // The IPNS name (peer ID)
    pub key_name: String, // Daemon key name (if NamedKeys mode)
}

pub struct IpnsResponse {
    pub value: Vec<u8>,
    pub cid: String,
    pub ipns_path: String,
    pub resolved_at: SystemTime,
}

pub struct KeyInfo {
    pub name: String,
    pub peer_id: String,
    pub user_key_hash: String,  // For reverse lookup
}
```

#### Size Limits

- Immutable value: Practical limit ~1-10 MB (network/daemon constraints)
- No hard protocol limit, but larger values may fail to propagate
- IPNS record size: Small (just points to CID)

### 2.2 Implementation Plan

#### Phase 1: Core Infrastructure

1. **Key Derivation Module** (`ipfs/key_derivation.rs`)
   - Hash user key → deterministic key name
   - IPFS `key_gen` wrapper with collision detection
   - Key listing and lookup by user key hash

2. **Key Cache** (`ipfs/key_cache.rs`)
   - In-memory cache: user_key → (key_name, peer_id)
   - Persistent cache option (serde JSON file)
   - Thread-safe access (Arc<RwLock<HashMap>>)

3. **Value Management** (`ipfs/value.rs`)
   - Add (upload) with size validation
   - Cat (download) with streaming support
   - Pin/unpin helpers

#### Phase 2: Basic Put/Get Operations

4. **Put Implementation** (`ipfs/put.rs`)
   - Derive/lookup IPNS key name
   - Add value → get CID
   - Publish IPNS name → CID
   - Optional pinning
   - Return PutReceipt

5. **Get Implementation** (`ipfs/get.rs`)
   - Derive/lookup IPNS key name
   - Resolve IPNS name → CID (with polling/retry)
   - Cat CID → bytes
   - Cache resolved CID for TTL period

#### Phase 3: Advanced Features

6. **Polling & Resolution**
   - Configurable retry logic for IPNS resolution
   - Exponential backoff
   - Local-first resolution (cache)

7. **Garbage Collection Integration**
   - Track pinned CIDs per user key
   - Unpin old CID when updating
   - Ref-counted pins for shared values

8. **Batch Operations**
   - Multi-get (parallel cat after resolve)
   - Multi-put (parallel add, sequential publish)

#### Phase 4: Testing & Refinement

9. **Integration Tests**
   - Immutable roundtrip (already exists ✅)
   - IPNS mutable roundtrip (already exists ✅)
   - High-level KV API tests
   - Concurrent update tests
   - Key collision tests
   - Large value tests (>1 MB)

10. **Documentation**
    - API docs with examples
    - IPNS vs immutable tradeoffs
    - Daemon configuration requirements
    - Performance tuning (gateway caching, etc.)

### 2.3 Test Coverage Strategy

Based on existing tests:

- ✅ `ipfs_immutable_roundtrip.rs` - Validates basic add/cat
- ✅ `ipfs_mutable_roundtrip.rs` - Validates IPNS publish/resolve with key_gen
- **New tests needed:**
  - `ipfs_kv_basic.rs` - High-level API roundtrip
  - `ipfs_kv_update.rs` - Sequential updates to same key
  - `ipfs_kv_cache.rs` - Key cache behavior
  - `ipfs_kv_large_values.rs` - Multi-MB values
  - `ipfs_kv_gc.rs` - Pin/unpin lifecycle

### 2.4 Security & Operational Considerations

**Privacy:**
- Content is public and discoverable by CID
- IPNS names are public (peer IDs)
- DHT lookups are visible to network
- Consider encrypting values at application layer

**Authentication:**
- IPNS updates authenticated via ed25519 signatures
- Only private key holder can update IPNS name
- CID immutability provides integrity

**Durability:**
- Requires pinning for persistence
- Unpinned content subject to garbage collection
- IPNS records expire (default 24h, must re-publish)
- Consider using pinning services for production

**Performance:**
- Add latency: 10ms-1s (local daemon)
- IPNS resolve: 1-10s (DHT propagation)
- Cat latency: 10ms-5s (depending on providers)
- Resolution caching critical for read-heavy workloads

**Dependencies:**
- Requires running Kubo daemon (or compatible IPFS node)
- Default RPC endpoint: http://127.0.0.1:5001
- Network connectivity required for DHT operations

---

## 3. Unified API Considerations

### 3.1 Common Traits

Both implementations could implement shared traits:

```rust
pub trait KvStore: Send + Sync {
    async fn put(&self, key: &[u8], value: &[u8]) -> Result<String>;
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    async fn delete(&self, key: &[u8]) -> Result<()>;
}

// Mainline and IPFS each implement this trait
impl KvStore for MainlineDhtKv { /* ... */ }
impl KvStore for IpfsKv { /* ... */ }
```

### 3.2 Abstraction Layers

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

### 3.3 Tradeoffs Summary

| Feature          | Mainline DHT           | IPFS                               |
| ---------------- | ---------------------- | ---------------------------------- |
| Max value size   | ~1 KB                  | ~1-10 MB (practical)               |
| Get latency      | 100ms-5s               | 1-10s (IPNS), 10ms-1s (immutable)  |
| Put latency      | 1-5s                   | 10ms-1s (add), 1-5s (IPNS publish) |
| Durability       | Temporary (hours-days) | Requires pinning                   |
| Dependencies     | None (pure DHT)        | Kubo daemon                        |
| Network usage    | UDP                    | TCP/QUIC/WebSocket                 |
| Privacy          | Public                 | Public                             |
| Auth model       | ed25519 signatures     | ed25519 signatures (IPNS)          |
| Update semantics | CAS (seq numbers)      | Last-write (IPNS publish)          |

---

## 4. Implementation Sequencing

Recommended order:

1. **Mainline DHT KV** (simpler, fewer dependencies)
   - Leverage existing test patterns
   - Build key derivation module
   - Implement basic put/get
   - Add CAS semantics
   - Document and test

2. **IPFS KV** (more complex, daemon-dependent)
   - Leverage existing IPNS test patterns
   - Build key cache system
   - Implement IPNS-based put/get
   - Add pinning lifecycle
   - Document and test

3. **Unified Interface** (optional)
   - Define common traits
   - Implement backend abstraction
   - Add backend-specific configuration
   - Integration tests across both backends

---

## 5. Testing Requirements

### 5.1 Mainline DHT Tests

- [ ] Basic put/get roundtrip (testnet)
- [ ] Basic put/get roundtrip (mainnet, ignored)
- [ ] Concurrent updates with CAS
- [ ] Salt isolation (multiple values, one key)
- [ ] Size limit enforcement
- [ ] Key derivation consistency
- [ ] Polling timeout behavior
- [ ] Network partition recovery

### 5.2 IPFS Tests

- [ ] Basic put/get roundtrip (immutable)
- [ ] Basic put/get roundtrip (IPNS mutable)
- [ ] Sequential updates (IPNS re-publish)
- [ ] Key cache hit/miss
- [ ] Large value handling (>1 MB)
- [ ] Pin lifecycle (add, update, unpin)
- [ ] Garbage collection interaction
- [ ] Daemon connection failure handling
- [ ] IPNS resolution timeout

### 5.3 Integration Tests

- [ ] Backend switching (same interface)
- [ ] Cross-backend data migration scenarios
- [ ] Performance benchmarks (latency, throughput)
- [ ] Concurrent client tests

---

## 6. Documentation Deliverables

For each API:

1. **API Reference** - Rustdoc with examples
2. **User Guide** - Setup, usage patterns, best practices
3. **Security Guide** - Threat model, key management, encryption
4. **Performance Guide** - Tuning, caching, batch operations
5. **Migration Guide** - Switching backends, data portability

---

## Notes for AI Agents

When implementing:

- Start with immutable storage tests as foundation
- Build mutable storage on top (both DHT and IPNS)
- Validate all size limits before network operations
- Implement polling with exponential backoff everywhere
- Cache aggressively (derived keys, resolved values)
- Provide clear error messages for common failures (daemon down, timeout, size exceeded)
- Follow existing test patterns (testnet + ignored mainnet)
- Run `cargo +nightly fmt` only in `hubert/` crate after edits
- Run `cargo clippy` in `hubert/` before ending turn
- Do not stage or commit without direction
