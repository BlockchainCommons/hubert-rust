# Server Implementation for Hubert

This document describes the HTTP server implementation added to Hubert.

## Overview

The `server` command starts an HTTP server that accepts PUT and GET requests for storing and retrieving envelopes. This provides a centralized option for applications that prefer HTTP communication over direct DHT or IPFS access.

## Architecture

```
Client (ServerKv) <--HTTP POST--> Server <--Memory--> In-Memory Storage
```

### Components

1. **`src/server/mod.rs`** - Module exports
2. **`src/server/server.rs`** - HTTP server implementation
3. **`src/server/kv.rs`** - Client implementation (ServerKv)
4. **`src/server/error.rs`** - Error types

## Server Implementation

### Starting the Server

```bash
# Start server on default port (45678)
hubert server

# Start server on custom port
hubert server --port 8080
```

### Configuration

```rust
pub struct ServerConfig {
    pub port: u16,
    pub max_ttl: u64,
}
```

Default configuration:
- **Port**: 45678
- **Max TTL**: 86400 seconds (24 hours)

### Storage Model

- **In-memory storage** using `HashMap<ARID, StorageEntry>`
- **Write-once semantics** - duplicate ARIDs rejected with 409 Conflict
- **Mandatory TTL** - all entries expire (hubert is for coordination, not long-term storage)
  - If `put()` specifies TTL > max_ttl, it's clamped to max_ttl
  - If `put()` specifies `None`, max_ttl is used
- **Thread-safe** - uses `Arc<RwLock<HashMap>>` for concurrent access
### Storage Format

Envelopes are stored as **UR strings** (not as `Envelope` objects) to avoid `Rc`/`Arc` issues:

```rust
struct StorageEntry {
    envelope_ur: String,  // Stored as "ur:envelope/..."
    expires_at: Option<Instant>,
}
```

This approach:
- Avoids Send/Sync issues with `Envelope` (uses `Arc` internally when `multithreaded` feature is enabled)
- Allows serialization/persistence in the future
- Maintains deterministic representation

## HTTP API

### PUT Endpoint

**URL:** `POST /put`

**Request Body Format:**
```
Line 1: ur:arid/<arid-data>
Line 2: ur:envelope/<envelope-data>
Line 3 (optional): <ttl-seconds>
```

**Example:**
```
ur:arid/hdcxjelehfmtuoosqzjypfgasbntjlsnihrhgepsdensolzmhgfyfzcptydeknatfmnloncmadva
ur:envelope/tpsoiyfdihjzjzjldmksbaoede
60
```

**Responses:**
- `200 OK` - Envelope stored successfully
- `400 Bad Request` - Invalid request format
- `409 Conflict` - ARID already exists

### GET Endpoint

**URL:** `POST /get`

**Request Body Format:**
```
ur:arid/<arid-data>
```

**Responses:**
- `200 OK` - Returns envelope as UR string in body
- `400 Bad Request` - Invalid request format
- `404 Not Found` - ARID not found or expired

## Client Implementation (ServerKv)

The `ServerKv` struct implements the `KvStore` trait:

```rust
use hubert::{KvStore, server::ServerKv};

let store = ServerKv::new("http://127.0.0.1:45678");

// Put envelope
store.put(&arid, &envelope).await?;

// Put with TTL
store.put_with_ttl(&arid, &envelope, 60).await?;

// Get envelope
if let Some(envelope) = store.get(&arid).await? {
    // Process envelope
}
```

### Features

- Implements full `KvStore` trait
- Automatic UR encoding/decoding
- HTTP error handling
- Optional TTL support via `put_with_ttl()`

## Dependencies

### Added to Cargo.toml

```toml
[dependencies]
# HTTP server
axum = "0.7"
tokio = { version = "1", features = ["sync", "macros", "rt-multi-thread"] }

# HTTP client (for ServerKv)
reqwest = "0.12"

# Async trait
async-trait = "0.1"

# Enable Arc for Envelope (thread-safe)
bc-envelope = { version = "^0.34.0", features = ["multithreaded"] }
dcbor = { version = "^0.23.0", features = ["multithreaded"] }
```

The `multithreaded` feature switches `Envelope` from `Rc` to `Arc`, making it `Send + Sync`.

## Testing

### In-Process Tests

Located in `tests/test_server.rs`:

1. **`test_server_put_get_roundtrip`** - Basic put/get cycle
2. **`test_server_write_once`** - Write-once semantics enforcement
3. **`test_server_get_nonexistent`** - Non-existent ARID handling
4. **`test_server_ttl`** - TTL expiration verification

All tests spawn the server in-process using `tokio::spawn` and communicate via HTTP.

### Running Tests

```bash
# Run all server tests
cargo test --test test_server

# Run specific test
cargo test --test test_server test_server_put_get_roundtrip
```

**Test Results:** All 4 tests pass in ~2 seconds

## CLI Integration

### New Command

```bash
hubert server [--port PORT]
```

### Updated Help

```
Commands:
  generate  Generate a new ARID
  put       Store an envelope at an ARID
  get       Retrieve an envelope by ARID
  check     Check if storage backend is available
  server    Start the Hubert HTTP server
  help      Print this message or the help of the given subcommand(s)
```

## Error Handling

### Server Errors

```rust
pub enum ServerError {
    BadRequest(String),
    NotFound,
    Conflict(String),
    InternalError(String),
}
```

Each error maps to appropriate HTTP status codes via `IntoResponse`.

### Client Errors

```rust
pub enum PutError {
    AlreadyExists,
    ServerError(String),
    NetworkError(reqwest::Error),
}

pub enum GetError {
    NotFound,
    ServerError(String),
    NetworkError(reqwest::Error),
}
```

## Security Considerations

### Current Implementation

- **No authentication** - Open access
- **No encryption** - Plain HTTP
- **No rate limiting**
- **In-memory only** - Data lost on restart
- **Coordination-focused** - All entries expire (max 24h default); not for long-term storage

### Future Enhancements

Potential improvements:
- TLS/HTTPS support
- API key authentication
- Rate limiting per IP
- Persistent storage (file/database)
- Clustering/replication
- Metrics/monitoring

## Performance

### Benchmarks

Not yet measured, but expected characteristics:
- **Latency:** Low (in-memory, no disk I/O)
- **Throughput:** High (async, no blocking)
- **Capacity:** Limited by RAM

### Limitations

- **Memory bound** - All entries held in RAM
- **No persistence** - Restart loses all data
- **No clustering** - Single server instance
- **TTL cleanup** - Lazy (only on access)

## Future Work

1. **Persistence** - Add file or database backend
2. **Authentication** - Add API key or OAuth support
3. **Clustering** - Support multiple server instances
4. **Metrics** - Prometheus/StatsD integration
5. **Admin API** - List/delete entries, health checks
6. **WebSocket** - Real-time notifications
7. **Compression** - Gzip response bodies
8. **CORS** - Cross-origin support for web clients

## Example Usage

### Start Server

```bash
# Terminal 1: Start server
hubert server --port 45678
```

### Use Client

```rust
use bc_components::ARID;
use bc_envelope::Envelope;
use hubert::{KvStore, server::ServerKv};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = ServerKv::new("http://127.0.0.1:45678");

    let arid = ARID::new();
    let envelope = Envelope::new("Hello, Server!");

    // Store with 1 hour TTL
    client.put_with_ttl(&arid, &envelope, 3600).await?;

    // Retrieve
    if let Some(env) = client.get(&arid).await? {
        println!("Retrieved: {}", env);
    }

    Ok(())
}
```

### Via HTTP Directly

```bash
# Generate ARID and envelope
ARID=$(hubert generate arid)
ENVELOPE=$(envelope subject type string "Test message")

# PUT request
curl -X POST http://127.0.0.1:45678/put \
  -d "$ARID
$ENVELOPE
3600"

# GET request
curl -X POST http://127.0.0.1:45678/get \
  -d "$ARID"
```

## Summary

The server implementation provides:
- ✅ HTTP API for put/get operations
- ✅ In-memory storage with mandatory TTL (max 24h default)
- ✅ Write-once semantics
- ✅ Thread-safe concurrent access
- ✅ Full `KvStore` trait implementation
- ✅ Comprehensive test coverage
- ✅ CLI integration
- ✅ Zero clippy warnings
- ✅ Coordination-focused design (not for long-term storage)

This makes Hubert usable as a centralized envelope coordination service while maintaining compatibility with the existing KvStore abstraction.
