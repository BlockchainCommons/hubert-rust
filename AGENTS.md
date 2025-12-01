# Hubert: Distributed Key-Value Store

Hubert provides write-once key-value storage using BitTorrent mainline DHT and IPFS with ARID-based addressing.

**For usage examples and API documentation, see:**
- [`docs/APIManual.md`](docs/APIManual.md) - API usage, code examples, KvStore trait
- [`docs/CLIManual.md`](docs/CLIManual.md) - CLI usage, storage backends, core concepts

## Design Philosophy

**Write-Once Semantics:**
- Each ARID key is written exactly once by the putter
- No support for updates, versioning, or multiple values per ARID
- Putter distributes ARID to getters via external means (out-of-band)
- Simplified API with no CAS, sequence numbers, or conflict resolution

**Envelope-Based Values:**
- All values are Gordian Envelopes (`bc_envelope::Envelope`)
- Deterministic dCBOR serialization for network transport
- Native support for encryption, compression, signatures, and elision

**Key Distribution Model:**
- Putter shares ARID with getters (QR code, envelope, secure channel, etc.)
- ARID acts as lookup capability; application-layer encryption provides access control

## Architecture Overview

### 1. BitTorrent Mainline DHT (MainlineDhtKv)

Uses BEP-44 mutable storage for write-once ARID-keyed envelopes:

- **Key Derivation**: ARID → ed25519 via HKDF-HMAC-SHA-256
  - Salt: `b"hubert-mainline-dht-v1"`
  - Deterministic: same ARID always produces same DHT location
- **Storage**: BEP-44 mutable items (seq=1, salt=None, write-once)
- **Size Limit**: ≤1000 bytes after dCBOR serialization
- **Dependencies**: None (embedded DHT client)
- **Latency**: 100ms-5s for get operations
- **Durability**: Temporary (hours to days, no re-publication)
- **Obfuscation**: All payloads are obfuscated using ChaCha20 with an ARID-derived key, making stored data appear as uniform random bytes

### 2. IPFS (IpfsKv)

Uses IPNS for write-once ARID-keyed envelopes:

- **Key Derivation**: ARID → IPNS key name `format!("hubert-{}", arid.hex())`
  - Salt: `b"hubert-ipfs-ipns-v1"`
  - IPNS name derived from ed25519 keypair
- **Storage**: IPNS name → CID → envelope bytes
- **Size Limit**: ~1-10 MB practical limit
- **Dependencies**: Kubo daemon (http://127.0.0.1:5001)
- **Latency**: 1-10s for IPNS resolution
- **Durability**: Requires pinning for persistence
- **Obfuscation**: All payloads are obfuscated using ChaCha20 with an ARID-derived key, making stored data appear as uniform random bytes

### 3. Hybrid Storage (HybridKv)

Combines DHT and IPFS with automatic size-based routing:

- **Small envelopes (≤1000 bytes)**: Direct DHT storage (obfuscated by DHT layer)
- **Large envelopes (>1000 bytes)**: Reference envelope in DHT → actual envelope in IPFS
  - Actual envelope stored in IPFS with new reference ARID (obfuscated by IPFS layer)
  - Reference envelope stored in DHT with original ARID (obfuscated by DHT layer)
- **Reference Envelope Format**:
  ```
  '' [
      'dereferenceVia': "ipfs",
      'id': <reference_ARID>,
      'size': <envelope_size>
  ]
  ```
- **Transparent**: Caller doesn't need to know which backend is used
- **Obfuscation**: Both layers obfuscate their payloads independently, all data appears as uniform random bytes

**Comparison:**

| Feature      | MainlineDhtKv       | IpfsKv           | HybridKv                        |
| ------------ | ------------------- | ---------------- | ------------------------------- |
| Max size     | ~1 KB               | ~1-10 MB         | ~1-10 MB (automatic fallback)   |
| Get latency  | 100ms-5s            | 1-10s            | 100ms-5s (small), 1-15s (large) |
| Dependencies | None                | Kubo daemon      | Kubo (for large only)           |
| Durability   | Temporary           | Pinning required | Mixed                           |
| **Best for** | Small, fast lookups | Large payloads   | Automatic optimization          |

## ARID Key Management

### ARID Properties

From `bc_components::ARID`:
- Fixed 32-byte (256-bit) cryptographically strong identifier
- Statistically random, non-correlatable bits
- Suitable as input to cryptographic constructs (like HKDF)

### HKDF Derivation Details

Both backends use HKDF-HMAC-SHA-256 from `bc_crypto`:

```rust
// Mainline DHT derivation
fn derive_mainline_signing_key(arid: &ARID) -> [u8; 32] {
    let salt = b"hubert-mainline-dht-v1";
    let seed = hkdf_hmac_sha256(arid.as_bytes(), salt, 32);
    // ... convert to ed25519 signing key
}

// IPFS IPNS derivation (for key name)
fn derive_ipfs_key_name(arid: &ARID) -> String {
    format!("hubert-{}", arid.hex())
}
```

### Salt Management

**Default Salts:**
- Mainline DHT: `b"hubert-mainline-dht-v1"`
- IPFS IPNS: `b"hubert-ipfs-ipns-v1"`
- Hybrid: Uses DHT salt for primary, IPFS salt for references

**Purpose:**
- Domain separation between different applications/versions
- Prevents cross-protocol key reuse
- Versioned for protocol evolution

### Key Derivation Guarantees

1. **Determinism**: Same ARID + same salt → same derived key (always)
2. **Isolation**: Different salts → statistically independent keys
3. **Irreversibility**: Cannot derive ARID from public key or DHT key
4. **Collision Resistance**: Cryptographic strength from HKDF-HMAC-SHA-256
5. **Cross-Network Isolation**: Mainline and IPFS keys are independent

### Security Considerations

**ARID Storage:**
- Users must securely store ARIDs to retain write capability
- Loss of ARID = loss of ability to prove authorship
- Getters only need ARID for read access (ARID as capability)

**Write-Once Guarantees:**
- Once written, value cannot be updated by anyone (including original putter)
- AlreadyExists errors prevent accidental overwrites
- Integrity protected by cryptographic signatures

**Network Visibility:**
- Public keys are visible on DHT/IPFS networks
- ARID itself is NOT published (only derived public keys)
- Envelope bytes are visible to network (opaque binary data)
- Use envelope `.encrypt()` for confidentiality at application layer

**Key Distribution:**
- ARID distribution is out-of-band (application responsibility)
- ARID acts as read capability (bearer token)
- Secure channel recommended for ARID sharing (Signal, envelope, QR)


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
