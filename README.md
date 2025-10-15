# Hubert: Secure Distributed Substrate for Multiparty Transactions

Hubert provides a distributed infrastructure for secure multiparty transactions, such as FROST threshold signature ceremonies, enabling participants to communicate bidirectionally with complete opacity to outsiders. By leveraging write-once distributed storage with cryptographic identifiers, Hubert creates a trustless coordination layer where parties can exchange encrypted messages without relying on centralized servers or exposing sensitive information to network observers.

## Primary Purpose

Hubert's main purpose is to facilitate **secure multiparty transactions** where:

- **Participants write once** using ARID (Apparently Random Identifier) keys
- **Messages contain ARIDs** for expected responses, enabling bidirectional communication
- **Complete opacity** to outsiders through end-to-end encryption (GSTP)
- **No central server** required for coordination
- **Trustless operation** using public distributed networks (BitTorrent DHT, IPFS)

**Example Use Case: FROST Signing Ceremony**
1. Coordinator publishes encrypted signing request with ARID for responses
2. Participants retrieve request, generate signature shares
3. Each participant publishes encrypted response at coordinator-specified ARID
4. Coordinator retrieves all responses and completes signature
5. Network observers see only GSTP envelopes (encrypted subject, sealed recipient assertions)
6. ARIDs are never exposed - shared privately via secure channels (Signal, QR codes, etc.)

## About the "Hubert" Name

Ted Nelson’s Project Xanadu had its own playful jargon. The basic object that behaved like a “file” was called a **bert**—named after **Bertrand Russell**. And because geeks can’t resist wordplay, there was also an **ernie**, the metered unit of billing in the publishing system.

Mark S. Miller, one of Xanadu’s architects, later designed the **Club System** (early groundwork for his capability-security thinking), which modeled group permissions but still relied on identity-checked ACLs rather than pure capabilities. That historical thread matters because Hubert sits exactly where Xanadu’s ideas were pointing, but finishes the job with cryptography.

So: **Hubert** is the **hub of berts**. In Xanadu terms, it’s the rendezvous point where these file-like objects (and their successors) can meet, exchange sealed messages, and coordinate—without servers, accounts, or trusted intermediaries. It’s a deliberate nod to Nelson’s vocabulary and to the “clubs” lineage, reframed for an era where capability comes from math, not administrators.

There’s also a second layer to the name. Cryptography uses a stock cast—**Alice**, **Bob**, **Carol**, et al.—to illustrate protocols. **Hubert** joins that dramatis personae as the sturdy switchboard operator in the background: the dropbox, dead-drop, and message hub that keeps multiparty ceremonies moving while revealing nothing but ciphertext to the outside world.

## Key Capabilities

### 1. Write-Once Distributed Storage

Hubert provides APIs for three storage backends, all using write-once semantics:

- **BitTorrent Mainline DHT**: Fast, lightweight, serverless (≤1 KB messages)
- **IPFS**: Large capacity, content-addressed (up to 10 MB messages)
- **Hybrid**: Automatic optimization by size, combining DHT speed with IPFS capacity

Write-once semantics eliminate race conditions and ensure message immutability—once published, content cannot be modified or deleted by anyone, providing strong integrity guarantees.

### 2. Cryptographic Addressing (ARID)

All storage operations use ARIDs (Apparently Random Identifiers):

- **Deterministic**: Same ARID always maps to same storage location
- **Privacy-Preserving**: ARID never exposed publicly; only derived storage keys visible
- **Collision-Resistant**: 256-bit cryptographic strength
- **Capability-Based**: ARID holder can read; ARID creator can write (once)
- **Secure Distribution**: ARIDs shared via encrypted channels (GSTP, Signal, QR codes)

Participants generate ARIDs for their messages and embed ARIDs where they expect responses, creating a decentralized communication graph. Storage networks see only derived keys (via HKDF), never the ARIDs themselves.

### 3. End-to-End Encryption (GSTP)

Hubert is designed to work with GSTP (Gordian Sealed Transaction Protocol) messages:

- **Sender Authentication**: Cryptographic signatures prove message origin
- **Receiver Encryption**: Only intended recipients can decrypt content
- **Multi-Recipient**: Single message encrypted to multiple parties
- **Stateless Operation**: Encrypted State Continuations eliminate server-side sessions
- **Network Opacity**: Storage networks see only encrypted envelopes

With GSTP integration, even storage indirection is secured—when Hybrid storage uses references, the references themselves are encrypted to the same recipients as the payload.

### 4. Bidirectional Communication Pattern

Hubert enables request-response flows without direct connections:

**Requester Workflow:**
1. Generate response_arid for where responder should publish
2. Create request message containing response_arid (encrypted within GSTP)
3. Publish encrypted request at request_arid
4. Share request_arid with responder via secure channel (Signal, QR code, etc.)
5. Monitor response_arid for responder's reply

**Responder Workflow:**
1. Receive request_arid via secure channel
2. Retrieve GSTP envelope from storage (using derived key from request_arid)
3. Decrypt and process request (only if recipient)
4. Extract response_arid from decrypted request
5. Create encrypted response
6. Publish response at response_arid

This pattern supports complex multiparty protocols (threshold signatures, distributed key generation, secure voting) without requiring participants to be online simultaneously or maintain persistent connections.

### 5. Automatic Storage Optimization (Hybrid Mode)

The Hybrid storage backend automatically optimizes for size:

- **Small messages (≤1 KB)**: Stored directly in DHT for fast retrieval
- **Large messages (>1 KB)**: Reference stored in DHT, actual content in IPFS
- **Transparent indirection**: Applications use same API regardless of size
- **Secure references**: With GSTP, even references are encrypted to recipients

This enables applications to send compact control messages via DHT while supporting large payloads (key material, proofs, documents) via IPFS without changing code.

## Benefits

### For Application Developers

- **Simple API**: Single interface for DHT, IPFS, or Hybrid storage
- **No Server Infrastructure**: Leverage existing public networks
- **Built-in Security**: GSTP integration handles encryption and authentication
- **Flexible Message Size**: From tiny control messages to multi-megabyte payloads
- **Language Agnostic**: Rust implementation with C FFI for cross-language use

### For Protocol Designers

- **Asynchronous by Default**: Participants don't need to be online simultaneously
- **Censorship Resistant**: No central points of failure or control
- **Privacy Preserving**: Network observers cannot read message content or graph structure
  - ARIDs shared only via secure channels (never exposed to storage networks)
  - Storage networks see only derived keys and encrypted GSTP envelopes
  - Envelope structure reveals no participant information
- **Replay Protection**: Write-once prevents message modification or replay
- **Scalable**: Public DHT/IPFS networks handle millions of nodes

### For End Users

- **No Account Required**: No registration, authentication, or identity verification
- **Cross-Platform**: Works anywhere BitTorrent DHT or IPFS is available
- **Sovereign**: Users control their own keys and identifiers
- **Auditable**: All messages cryptographically verifiable
- **Cost-Free**: No fees for using public networks (may pin IPFS content locally)

## Usage

Hubert can be used both as a **Rust library** for building distributed applications and as a **command-line tool** for interactive storage operations.

### As a Rust Library

Add Hubert to your `Cargo.toml`:

```toml
[dependencies]
hubert = "0.1.0"
bc-components = "^0.25.0"
bc-envelope = "^0.34.0"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

#### Basic Example: Mainline DHT Storage

```rust
use bc_components::ARID;
use bc_envelope::Envelope;
use hubert::{KvStore, mainline::MainlineDhtKv};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a Mainline DHT store
    let store = MainlineDhtKv::new().await?;

    // Generate an ARID for this storage location
    let arid = ARID::new();

    // Create an envelope
    let envelope = Envelope::new("Hello, Hubert!");

    // Store the envelope (write-once)
    store.put(&arid, &envelope).await?;

    // Share the ARID with other parties via secure channel
    // (Signal, QR code, GSTP message, etc.)
    println!("ARID: {}", arid.ur_string());

    // Retrieve the envelope
    if let Some(retrieved) = store.get(&arid).await? {
        println!("Retrieved: {}", retrieved);
    }

    Ok(())
}
```

#### Example: IPFS Storage

```rust
use bc_components::ARID;
use bc_envelope::Envelope;
use hubert::{KvStore, ipfs::IpfsKv};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create an IPFS store (requires running Kubo daemon)
    let store = IpfsKv::new("http://127.0.0.1:5001");

    let arid = ARID::new();
    let envelope = Envelope::new("Large data payload");

    // Store using IPFS (supports up to 10 MB)
    store.put(&arid, &envelope).await?;

    // Retrieve
    if let Some(retrieved) = store.get(&arid).await? {
        println!("Retrieved from IPFS: {}", retrieved);
    }

    Ok(())
}
```

#### KvStore Trait

Both `MainlineDhtKv` and `IpfsKv` implement the `KvStore` trait:

```rust
use bc_components::ARID;
use bc_envelope::Envelope;
use hubert::KvStore;

async fn store_envelope(
    store: &impl KvStore,
    arid: &ARID,
    envelope: &Envelope,
) -> Result<(), Box<dyn std::error::Error>> {
    store.put(arid, envelope).await?;
    Ok(())
}
```

This allows you to write storage-backend-agnostic code and swap implementations as needed.

#### Write-Once Semantics

All storage backends enforce write-once semantics. Attempting to write to an existing ARID will fail:

```rust
use hubert::mainline::PutError;

// First write succeeds
store.put(&arid, &envelope1).await?;

// Second write to same ARID fails
match store.put(&arid, &envelope2).await {
    Err(PutError::AlreadyExists { .. }) => {
        println!("ARID already exists");
    }
    _ => {}
}
```

#### Error Handling

Each backend has specific error types:

```rust
use hubert::mainline::{PutError, GetError};

match store.put(&arid, &envelope).await {
    Ok(_) => println!("Stored successfully"),
    Err(PutError::AlreadyExists { key }) => {
        println!("Key {} already exists", key);
    }
    Err(PutError::ValueTooLarge { size }) => {
        println!("Value too large: {} bytes", size);
    }
    Err(e) => println!("Error: {}", e),
}
```

### As a Command-Line Tool

Hubert includes a `hubert` CLI for storing and retrieving Gordian Envelopes using distributed storage backends.

#### Installation

```bash
# From source
cd hubert
cargo install --path .

# Or run directly
cargo build --bin hubert
./target/debug/hubert --help
```

#### Usage

The CLI supports four commands:

**Generate an ARID**

```bash
# Generate a new ARID for use as a storage key
hubert generate arid
```

Example output: `ur:arid/hdcxjelehfmtuoosqzjypfgasbntjlsnihrhgepsdensolzmhgfyfzcptydeknatfmnloncmadva`

**Check Backend Availability**

```bash
# Check if Mainline DHT is available (default)
hubert check

# Check if IPFS daemon is running
hubert check --storage ipfs
```

**Store an Envelope**

```bash
# Store using Mainline DHT (default, ≤1 KB)
hubert put <ur:arid> <ur:envelope>

# Store using IPFS (up to 10 MB)
hubert put --storage ipfs <ur:arid> <ur:envelope>
```

Example:
```bash
# Generate an ARID and envelope
ARID=$(hubert generate arid)
ENVELOPE=$(envelope subject type string "Hello, Hubert!")

# Store the envelope
hubert put "$ARID" "$ENVELOPE"
```

**Retrieve an Envelope**

```bash
# Retrieve using Mainline DHT (default)
hubert get <ur:arid>

# Retrieve using IPFS
hubert get --storage ipfs <ur:arid>
```

The retrieved envelope is output in `ur:envelope` format.

#### CLI Options

- `--storage`, `-s`: Choose storage backend
  - `mainline` (default): BitTorrent Mainline DHT (fast, ≤1 KB messages)
  - `ipfs`: IPFS (large capacity, up to 10 MB messages)

#### Requirements

- **Mainline DHT**: No external daemon required
- **IPFS**: Requires running Kubo daemon at `127.0.0.1:5001`

## Architecture Overview

```
Application Layer
├── FROST Signing Ceremony
├── Distributed Key Generation
├── Secure Voting
└── Encrypted Messaging

GSTP Layer (Gordian Sealed Transaction Protocol)
├── Message Encryption (to recipients)
├── Message Signing (by sender)
├── Encrypted State Continuations
└── Request/Response Pairing

Hubert Storage Layer
├── ARID-Based Addressing
├── Write-Once Semantics
├── Size-Based Routing (Hybrid)
└── Backend Selection

Storage Backends
├── Mainline DHT (fast, ≤1 KB)
├── IPFS (large, pinned)
└── Hybrid (automatic)
```

## Use Cases

### 1. FROST Threshold Signatures

Coordinate multi-party threshold signature ceremonies:
- Coordinator publishes signing request with participant ARIDs
- Participants publish signature shares at designated ARIDs
- Coordinator aggregates shares into final signature
- No trusted server required; all communication encrypted

### 2. Distributed Key Generation (DKG)

Bootstrap threshold key generation among multiple parties:
- Participants exchange commitments via encrypted messages
- Share verification happens asynchronously
- Final key shares distributed to all participants
- Entire ceremony verifiable but private

### 3. Secure Multiparty Computation

Enable privacy-preserving computation across parties:
- Input commitments published by each party
- Computation steps coordinated via message passing
- Results revealed only to designated recipients
- Network observers see only encrypted data flow

### 4. Asynchronous Negotiations

Facilitate multi-round negotiations without real-time requirements:
- Parties exchange proposals at their convenience
- Each round builds on previous ARIDs
- State carried in encrypted continuations
- No intermediary servers involved

### 5. Decentralized Messaging

Provide sovereign messaging infrastructure:
- Sender publishes encrypted message at chosen ARID
- Shares ARID with recipient via secure channel (Signal, GSTP, QR code)
- Recipient retrieves and decrypts message (using derived storage key)
- No metadata exposed to network observers (only GSTP envelope structure visible)

## Security Model

**Threat Model:**
- Network observers can see derived storage keys and GSTP envelope structure
- Network observers see encrypted subject and `hasRecipient: SealedMessage` assertions
- Network observers cannot see ARIDs (stretched via HKDF to derive storage keys)
- Network observers cannot decrypt messages or determine recipients
- Storage networks cannot modify published messages (write-once)
- Only intended recipients can decrypt message content
- Only ARID creator can publish at that ARID (enforced by key derivation)

**Trust Assumptions:**
- Public DHT/IPFS networks are available and honest-majority
- Cryptographic primitives (ed25519, HKDF, AES-GCM) are secure
- Participants protect their private keys and ARIDs
- ARID distribution happens over secure channels (GSTP, Signal, QR codes)

**Security Properties:**
- **Confidentiality**: End-to-end encryption via GSTP; ARIDs never exposed publicly
- **Integrity**: Cryptographic signatures and write-once storage
- **Authentication**: Sender signatures verified by recipients
- **Availability**: Distributed storage resilient to node failures
- **Privacy**: Network metadata reveals only GSTP envelope structure, not content or ARIDs

## Getting Started

```rust
use hubert::prelude::*;
use bc_components::ARID;
use gstp::prelude::*;

// Choose storage backend
let hybrid_kv = HybridKv::new(dht, ipfs);

// Create GSTP message
let request_arid = ARID::new();
let response_arid = ARID::new();

let sealed_request = SealedRequest::new(function, request_arid, sender_xid)
    .with_parameter("response_arid", response_arid)
    .with_parameter("data", payload)
    .seal(vec![&recipient_xid], sender_keys)?;

// Store encrypted request
hybrid_kv.put(&request_arid, &sealed_request.into(), options).await?;

// Share request_arid with recipient via secure channel (Signal, QR code, etc.)
// ARID is never exposed to storage network - only derived key is visible

// Recipient retrieves, decrypts, processes, and publishes response at response_arid

// Retrieve encrypted response
if let Some(envelope) = hybrid_kv.get(&response_arid, options).await? {
    let sealed_response = SealedResponse::try_from(envelope)?;
    let response = sealed_response.unseal(sender_keys)?;
    // Process response
}
```

## Project Status

Hubert is currently in the design and specification phase. This document reflects the planned architecture and capabilities. Implementation is forthcoming.

See `AGENTS.md` for detailed technical specifications, API designs, and implementation plans.

## Contributing

Hubert is part of [Blockchain Commons](https://www.blockchaincommons.com/)' suite of technologies for secure, decentralized systems. We welcome contributions, feedback, and collaboration.

## License

Licensed under the BSD-2-Clause Plus Patent License.

## Related Projects

- **[Gordian Envelope](https://github.com/BlockchainCommons/bc-envelope-rust)**: Structured data format with encryption and signing
- **[GSTP](https://github.com/BlockchainCommons/gstp-rust)**: Sealed transaction protocol for secure messaging
- **[Clubs](https://github.com/BlockchainCommons/clubs-rust)**: Gordian Clubs
- **[bc-components](https://github.com/BlockchainCommons/bc-components-rust)**: Cryptographic components including ARID

---

**Hubert**: Enabling trustless coordination for secure multiparty transactions.
