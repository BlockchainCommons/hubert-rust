# Hubert Key-Value Storage API Manual

This document provides an overview of the key-value storage API provided by the Hubert crate. Hubert supports multiple storage backends, including Mainline DHT, IPFS, and Hybrid storage, through a common `KvStore` trait.

## Basic Example: Mainline DHT Storage

```rust
use bc_components::ARID;
use bc_envelope::Envelope;
use hubert::{KvStore, mainline::MainlineDhtKv};

#[tokio::main]
async fn main() -> hubert::Result<()> {
    // Create a Mainline DHT store
    let store = MainlineDhtKv::new().await?;

    // Generate an ARID for this storage location
    let arid = ARID::new();

    // Create an envelope
    let envelope = Envelope::new("Hello, Hubert!");

    // Store the envelope (write-once)
    // Parameters: arid, envelope, ttl_seconds (ignored for DHT), verbose
    let receipt = store.put(&arid, &envelope, None, false).await?;
    println!("Stored: {}", receipt);

    // Share the ARID with other parties via secure channel
    // (Signal, QR code, GSTP message, etc.)
    println!("ARID: {}", arid.ur_string());

    // Retrieve the envelope
    // Parameters: arid, timeout_seconds, verbose
    if let Some(retrieved) = store.get(&arid, None, false).await? {
        println!("Retrieved: {}", retrieved);
    }

    Ok(())
}
```

## Example: IPFS Storage

```rust
use bc_components::ARID;
use bc_envelope::Envelope;
use hubert::{KvStore, ipfs::IpfsKv};

#[tokio::main]
async fn main() -> hubert::Result<()> {
    // Create an IPFS store (requires running Kubo daemon)
    let store = IpfsKv::new("http://127.0.0.1:5001");

    let arid = ARID::new();
    let envelope = Envelope::new("Large data payload");

    // Store using IPFS (supports up to 10 MB)
    // TTL is used for IPNS record lifetime (default 24h if None)
    let receipt = store.put(&arid, &envelope, Some(86400), false).await?;
    println!("Stored: {}", receipt);

    // Retrieve with 10 second timeout
    if let Some(retrieved) = store.get(&arid, Some(10), false).await? {
        println!("Retrieved from IPFS: {}", retrieved);
    }

    Ok(())
}
```

## Example: Hybrid Storage

```rust
use bc_components::ARID;
use bc_envelope::Envelope;
use hubert::{KvStore, hybrid::HybridKv};

#[tokio::main]
async fn main() -> hubert::Result<()> {
    // Create a Hybrid store (combines DHT speed with IPFS capacity)
    let store = HybridKv::new("http://127.0.0.1:5001").await?;

    // Small envelopes (≤1KB) go to DHT automatically
    let arid1 = ARID::new();
    let small = Envelope::new("Small message");
    store.put(&arid1, &small, None, false).await?;

    // Large envelopes (>1KB) use DHT reference + IPFS storage
    let arid2 = ARID::new();
    let large = Envelope::new("x".repeat(2000));
    store.put(&arid2, &large, None, false).await?;

    // Retrieval is transparent - same API for both
    let _retrieved1 = store.get(&arid1, None, false).await?;
    let _retrieved2 = store.get(&arid2, None, false).await?;

    Ok(())
}
```

## KvStore Trait

All storage backends implement the `KvStore` trait, which provides a unified interface:

```rust
#[async_trait::async_trait(?Send)]
pub trait KvStore: Send + Sync {
    /// Store an envelope at the given ARID (write-once).
    async fn put(
        &self,
        arid: &ARID,
        envelope: &Envelope,
        ttl_seconds: Option<u64>,
        verbose: bool,
    ) -> Result<String>;

    /// Retrieve an envelope by ARID with optional timeout.
    async fn get(
        &self,
        arid: &ARID,
        timeout_seconds: Option<u64>,
        verbose: bool,
    ) -> Result<Option<Envelope>>;

    /// Check if an ARID exists without fetching the envelope.
    async fn exists(&self, arid: &ARID) -> Result<bool>;
}
```

This allows you to write storage-backend-agnostic code:

```rust
use bc_components::ARID;
use bc_envelope::Envelope;
use hubert::{KvStore, Result};

async fn store_envelope(
    store: &impl KvStore,
    arid: &ARID,
    envelope: &Envelope,
) -> Result<String> {
    // Works with any backend: MainlineDhtKv, IpfsKv, HybridKv, etc.
    store.put(arid, envelope, None, false).await
}
```

### Parameters

**`put` method:**
- `arid`: The ARID key for this storage location
- `envelope`: The envelope to store
- `ttl_seconds`: Optional time-to-live
  - **Mainline DHT**: Ignored (no TTL support)
  - **IPFS**: IPNS record lifetime (default 24h if None)
  - **Hybrid**: Uses IPFS TTL for large envelopes
  - **Server**: Clamped to server's max_ttl; uses max_ttl if None
- `verbose`: Enable verbose logging with timestamps

**`get` method:**
- `arid`: The ARID key to retrieve
- `timeout_seconds`: Maximum time to poll for the envelope
  - If `None`, uses backend-specific default (typically 30s)
  - Returns `Ok(None)` if not found within timeout
- `verbose`: Enable verbose logging with polling dots

**`exists` method:**
- `arid`: The ARID key to check
- Returns `Ok(true)` if exists, `Ok(false)` otherwise

## Write-Once Semantics

All storage backends enforce write-once semantics. Attempting to write to an existing ARID will fail:

```rust
use hubert::{Error, Result};

// First write succeeds
store.put(&arid, &envelope1, None, false).await?;

// Second write to same ARID fails
match store.put(&arid, &envelope2, None, false).await {
    Err(Error::AlreadyExists { arid }) => {
        println!("ARID {} already exists", arid);
    }
    _ => {}
}
```

You can also check for existence before attempting to put:

```rust
if store.exists(&arid).await? {
    println!("ARID already in use");
} else {
    store.put(&arid, &envelope, None, false).await?;
}
```

## Error Handling

The library uses a unified `Error` type with backend-specific variants:

```rust
use hubert::Error;

match store.put(&arid, &envelope, None, false).await {
    Ok(receipt) => println!("Stored: {}", receipt),
    Err(Error::AlreadyExists { arid }) => {
        println!("ARID {} already exists", arid);
    }
    Err(Error::Mainline(e)) => {
        // Mainline-specific error (e.g., ValueTooLarge)
        println!("DHT error: {}", e);
    }
    Err(Error::Ipfs(e)) => {
        println!("IPFS error: {}", e);
    }
    Err(e) => println!("Error: {}", e),
}
```

### Common Error Variants

- `Error::AlreadyExists { arid }`: The ARID already has a stored value
- `Error::NotFound`: The requested ARID was not found
- `Error::InvalidArid`: The ARID format is invalid
- `Error::Mainline(e)`: Mainline DHT-specific error
  - `ValueTooLarge { size }`: Envelope exceeds 1KB limit
- `Error::Ipfs(e)`: IPFS-specific error
- `Error::Hybrid(e)`: Hybrid storage-specific error
- `Error::Envelope(e)`: Envelope serialization/deserialization error
- `Error::Cbor(e)`: CBOR encoding/decoding error

## Polling and Timeouts

The `get` method polls the storage backend until the envelope appears or the timeout is reached. This is useful for coordination between parties:

```rust
// Party A: Store envelope
let arid = ARID::new();
store.put(&arid, &envelope, None, false).await?;

// Share ARID with Party B via secure channel...

// Party B: Poll for envelope with 30 second timeout and verbose output
match store.get(&arid, Some(30), true).await? {
    Some(envelope) => {
        println!("Received envelope");
        // Process envelope...
    }
    None => {
        println!("Envelope not found within 30 seconds");
    }
}
```

When `verbose` is enabled, the get operation will print:
- Start time
- Polling dots (one per retry)
- Success/timeout message with elapsed time
