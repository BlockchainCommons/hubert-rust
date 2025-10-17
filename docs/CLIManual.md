# Hubert CLI Manual

## Table of Contents

- [Hubert CLI Manual](#hubert-cli-manual)
  - [Table of Contents](#table-of-contents)
  - [Introduction](#introduction)
  - [What is Hubert?](#what-is-hubert)
  - [Installation](#installation)
  - [Getting Started](#getting-started)
    - [Help](#help)
    - [Version](#version)
  - [Storage Backends](#storage-backends)
    - [Mainline DHT](#mainline-dht)
    - [IPFS](#ipfs)
    - [Hybrid](#hybrid)
    - [Server](#server)
  - [Core Concepts](#core-concepts)
    - [ARIDs: Apparently Random Identifiers](#arids-apparently-random-identifiers)
    - [Envelopes](#envelopes)
    - [Write-Once Semantics](#write-once-semantics)
  - [Basic Operations](#basic-operations)
    - [Generating an ARID](#generating-an-arid)
    - [Creating an Envelope](#creating-an-envelope)
    - [Storing Data (Put)](#storing-data-put)
    - [Retrieving Data (Get)](#retrieving-data-get)
    - [Checking Backend Availability](#checking-backend-availability)
  - [Storage Backend Examples](#storage-backend-examples)
    - [Using Mainline DHT](#using-mainline-dht)
    - [Using IPFS](#using-ipfs)
    - [Using Hybrid Storage](#using-hybrid-storage)
    - [Using Hubert Server](#using-hubert-server)
  - [Advanced Usage](#advanced-usage)
    - [Verbose Output](#verbose-output)
    - [Timeouts](#timeouts)
    - [IPFS Pinning](#ipfs-pinning)
    - [Server TTL](#server-ttl)
  - [Bidirectional Communication Pattern](#bidirectional-communication-pattern)
    - [Request-Response Flow](#request-response-flow)
  - [Integration with GSTP](#integration-with-gstp)
  - [Troubleshooting](#troubleshooting)
  - [Command Reference](#command-reference)

## Introduction

This manual provides a tutorial-style guide to using the `hubert` command-line tool. Hubert enables secure, distributed key-value storage for Gordian Envelopes, supporting multiple storage backends including BitTorrent Mainline DHT, IPFS, and centralized servers.

## What is Hubert?

Hubert is a secure distributed substrate for multiparty transactions. It provides:

- **Write-once distributed storage**: Data written once cannot be modified or deleted
- **Cryptographic addressing**: Uses ARIDs (Apparently Random Identifiers) as keys
- **Multiple storage backends**: BitTorrent DHT, IPFS, Hybrid, and Server modes
- **End-to-end encryption ready**: Designed to work with GSTP (Gordian Sealed Transaction Protocol)
- **No central authority**: Operates on public distributed networks

**Primary Use Case**: Facilitating secure multiparty transactions like FROST threshold signature ceremonies, where participants need to exchange encrypted messages without revealing sensitive information to network observers.

## Installation

Build and install `hubert` from the repository:

```
cd /path/to/hubert
cargo install --path .
```

## Getting Started

### Help

View the main help to see all available commands:

```
hubert --help

│ Hubert: Secure distributed key-value store for Gordian Envelopes
│
│ Usage: hubert [OPTIONS] <COMMAND>
│
│ Commands:
│   generate  Generate a new ARID
│   put       Store an envelope at an ARID
│   get       Retrieve an envelope by ARID
│   check     Check if storage backend is available
│   server    Start the Hubert HTTP server
│   help      Print this message or the help of the given subcommand(s)
│
│ Options:
│   -s, --storage <STORAGE>
│           Storage backend to use
│
│           Possible values:
│           - mainline: BitTorrent Mainline DHT (fast, ≤1 KB messages)
│           - ipfs:     IPFS (large capacity, up to 10 MB messages)
│           - hybrid:   Hybrid (automatic: DHT for small, IPFS for large)
│           - server:   Hubert HTTP server (centralized coordination)
│
│           [default: mainline]
│
│       --host <HOST>
│           Server/IPFS host (for --storage server or --storage ipfs)
│
│       --port <PORT>
│           Port (for --storage server, --storage ipfs, --storage hybrid, or server command)
│
│   -v, --verbose
│           Enable verbose logging
│
│   -h, --help
│           Print help (see a summary with '-h')
│
│   -V, --version
│           Print version
```

### Version

Check the installed version:

```
hubert --version

│ hubert 0.1.0
```

## Storage Backends

Hubert supports four storage backends, each with different characteristics:

### Mainline DHT

**BitTorrent Mainline DHT** is a serverless, distributed hash table with over 10 million nodes worldwide.

- **Speed**: Fast (typically 1-5 seconds)
- **Size limit**: ≤1 KB (after bencode encoding)
- **Availability**: No setup required, works out of the box
- **Persistence**: Best-effort, nodes may drop data
- **Privacy**: High - widely distributed across global network

**Best for**: Small control messages, coordination data

### IPFS

**InterPlanetary File System** provides content-addressed storage with large capacity.

- **Speed**: Moderate (depends on gateway availability)
- **Size limit**: Up to 10 MB (practical limit)
- **Availability**: Requires local IPFS daemon or gateway access
- **Persistence**: Good with pinning, excellent with paid pinning services
- **Privacy**: Moderate - data stored on IPFS nodes

**Best for**: Large payloads, documents, key material, proofs

### Hybrid

**Hybrid mode** automatically selects the optimal backend based on message size.

- **Small messages (≤1 KB)**: Stored directly in Mainline DHT
- **Large messages (>1 KB)**: Content stored in IPFS, reference in DHT
- **Transparent**: Applications use same API regardless of size
- **Optimized**: Fast retrieval for small messages, large capacity when needed

**Best for**: Applications with variable message sizes

### Server

**Hubert Server** provides centralized storage for testing and controlled deployments.

- **Speed**: Very fast (local network)
- **Size limit**: Configurable (typically no practical limit)
- **Availability**: Requires running Hubert server
- **Persistence**: Memory backed or database-backed (SQLite)
- **Privacy**: Low - centralized, single point of observation

**Best for**: Development, testing, controlled environments

## Core Concepts

### ARIDs: Apparently Random Identifiers

An ARID (Apparently Random Identifier) is a 256-bit cryptographic identifier that serves as a key in Hubert's storage system.

**Properties**:
- **Deterministic**: Same ARID always maps to same storage location
- **Privacy-preserving**: ARID itself never exposed publicly
- **Collision-resistant**: 256-bit cryptographic strength
- **Capability-based**: ARID holder can read; ARID creator can write (once)

**Distribution**: ARIDs are shared between parties via secure channels (encrypted messages, QR codes, Signal, etc.), never published to storage networks.

**Storage derivation**: Storage networks see only derived keys, not the ARIDs themselves.

### Envelopes

All data in Hubert is stored as **Gordian Envelopes** - a structured, extensible data format with:

- Deterministic dCBOR serialization
- Built-in encryption support
- Signature capabilities
- Merkle digest trees for integrity
- Selective disclosure (elision)

Envelopes are stored in networks as binary dCBOR, and represented in text in UR (Uniform Resource) format: `ur:envelope/...`

### Write-Once Semantics

Hubert enforces **write-once semantics**:

- Each ARID can be written to exactly once
- No updates or modifications after initial write
- Attempting to overwrite fails with an error
- Eliminates race conditions and ensures immutability

This guarantees message integrity - once published, content cannot be altered by anyone.

## Basic Operations

### Generating an ARID

Create a new ARID for use as a storage key:

```
ARID=$(hubert generate arid)
echo $ARID

│ ur:arid/hdcxwzendlkofxcygymkfyjpjynnaawpvlmugugwamntkbguaehkgrwyzmjzgwrstlrphycmprsn
```

### Creating an Envelope

For testing, you can generate a test envelope with random data:

```
hubert generate envelope 20 # Number of random bytes

│ ur:envelope/tpsoghtlrelfasknehndrehpvolrzmdyfdndpmkgdrgrrkdkpdhgmn
```

Or create a real envelope using the `envelope` CLI tool (from bc-envelope-cli):

```
ENVELOPE=$(envelope subject type string 'Hello, Hubert')
echo $ENVELOPE

│ ur:envelope/tpsojnfdihjzjzjldwcxfdkpidihjpjyoynyghtd
```

### Storing Data (Put)

Store an envelope at an ARID using the default storage backend (Mainline DHT). No output indicates success:

```
hubert put $ARID $ENVELOPE
```

**Important**: Each ARID can only be written once. Attempting to write again will fail:

```
hubert put $ARID $ENVELOPE

│ Error: ur:arid/hdcxwzendlkofxcygymkfyjpjynnaawpvlmugugwamntkbguaehkgrwyzmjzgwrstlrphycmprsn already exists
```

### Retrieving Data (Get)

Retrieve the envelope stored at an ARID:

```
hubert get $ARID

│ ur:envelope/tpsojnfdihjzjzjldwcxfdkpidihjpjyoynyghtd
```

You can pipe the output to the `envelope` tool to inspect the content:

```
hubert get $ARID | envelope format

│ "Hello, Hubert"
```

Or extract the string directly:

```
hubert get $ARID | envelope extract string

│ Hello, Hubert
```

### Checking Backend Availability

Before using a storage backend, verify it's available:

```
hubert check

│ ✓ Mainline DHT is available
```

Check other backends:

```
hubert check --storage ipfs

│ ✓ IPFS is available at http://127.0.0.1:5001
```

```
hubert check --storage server --host localhost --port 45678

│ ✓ Hubert server is available at 127.0.0.1:45678 (version 0.1.0)
```

If a backend is unavailable, you'll see an error:

```
hubert check --storage server --port 1234

│ Error: ✗ Server is not available at 127.0.0.1:1234: error sending request for url (http://127.0.0.1:1234/health)│
```

## Storage Backend Examples

### Using Mainline DHT

Mainline DHT is the default backend, requiring no setup:

```
# Generate identifiers
ARID=$(hubert generate arid)
ENVELOPE=$(envelope subject type string "DHT message")

# Store and retrieve
hubert put $ARID $ENVELOPE
hubert get $ARID

│ ur:envelope/tpsojkjyisinjkdpisjkisyaonwmdy
```

**Note**: DHT operations may take 1-20 seconds as the client bootstraps into the network and propagates data.

### Using IPFS

IPFS requires a running daemon. Start it first:

```
# In a separate terminal
ipfs daemon
```

Then use IPFS storage:

```
ARID=$(hubert generate arid)
ENVELOPE=$(envelope subject type string "IPFS message")

# Store with IPFS backend
hubert put --storage ipfs $ARID $ENVELOPE
hubert get --storage ipfs $ARID

│ ur:envelope/tpsojkjyisinjkdpiojkisyatbhdax
```

**IPFS Pinning**: By default, content is not pinned. Use `--pin` to ensure persistence. This example uses the `--verbose` flag to show detailed output:

```
ARID=$(hubert generate arid)
hubert put --storage ipfs --pin --verbose $ARID $ENVELOPE

│ [2025-10-17T10:18:11.940Z] Starting IPFS put operation
│ [2025-10-17T10:18:11.940Z] Envelope size: 17 bytes
│ [2025-10-17T10:18:11.940Z] Getting or creating IPNS key
│ [2025-10-17T10:18:11.983Z] Adding content to IPFS
│ [2025-10-17T10:18:12.021Z] Content CID: QmPUKGjPUbJQVb5PonRh21EtaNJ19UHXUNqoFmSovT4YpT
│ [2025-10-17T10:18:12.021Z] Pinning content
│ [2025-10-17T10:18:12.053Z] Publishing to IPNS (write-once check)
│ [2025-10-17T10:18:46.384Z] IPFS put operation completed
│ [2025-10-17T10:18:46.384Z] ✓ Stored envelope at ARID
```

### Using Hybrid Storage

Hybrid mode automatically optimizes storage based on message size:

```
# Small message (≤1 KB) - uses DHT
SMALL_MSG=$(envelope subject type string "Small")
ARID_SMALL=$(hubert generate arid)
hubert put --storage hybrid --verbose $ARID_SMALL $SMALL_MSG

│ [2025-10-17T10:31:11.622Z] Storing envelope in DHT (size ≤ 1000 bytes)
│ [2025-10-17T10:31:11.623Z] Starting Mainline DHT put operation
│ [2025-10-17T10:31:11.623Z] Envelope size: 10 bytes
│ [2025-10-17T10:31:11.623Z] Deriving DHT signing key from ARID
│ [2025-10-17T10:31:11.624Z] Checking for existing value (write-once check)
│ [2025-10-17T10:31:14.950Z] Creating mutable DHT item
│ [2025-10-17T10:31:14.951Z] Putting value to DHT
│ [2025-10-17T10:31:17.062Z] Mainline DHT put operation completed
│ [2025-10-17T10:31:17.063Z] ✓ Stored envelope at ARID
```

```
# Large message (>1 KB) - uses IPFS with DHT reference
LARGE_MSG=$(hubert generate envelope 2000) # 2000 random bytes
ARID_LARGE=$(hubert generate arid)
hubert put --storage hybrid --verbose $ARID_LARGE $LARGE_MSG

│ [2025-10-17T10:31:47.880Z] Envelope too large for DHT, using IPFS indirection
│ [2025-10-17T10:31:47.880Z] Storing actual envelope in IPFS with reference ARID: ur:arid/hdcxjkgrnscpftlkwzeegurfttdrbnreckpmlnknrygewtimmnolghwtnslurhmobkhphefxsepr
│ [2025-10-17T10:31:47.881Z] Starting IPFS put operation
│ [2025-10-17T10:31:47.881Z] Envelope size: 2007 bytes
│ [2025-10-17T10:31:47.881Z] Getting or creating IPNS key
│ [2025-10-17T10:31:47.957Z] Adding content to IPFS
│ [2025-10-17T10:31:47.992Z] Content CID: QmPhKbUrdNNZinq3FF8XwXYcVznb7xJgQx9RU3gZ6TnJV9
│ [2025-10-17T10:31:47.992Z] Publishing to IPNS (write-once check)
│ [2025-10-17T10:32:21.350Z] IPFS put operation completed
│ [2025-10-17T10:32:21.354Z] Storing reference envelope in DHT at original ARID
│ [2025-10-17T10:32:21.354Z] Starting Mainline DHT put operation
│ [2025-10-17T10:32:21.355Z] Envelope size: 72 bytes
│ [2025-10-17T10:32:21.355Z] Deriving DHT signing key from ARID
│ [2025-10-17T10:32:21.356Z] Checking for existing value (write-once check)
│ [2025-10-17T10:32:24.324Z] Creating mutable DHT item
│ [2025-10-17T10:32:24.324Z] Putting value to DHT
│ [2025-10-17T10:32:26.463Z] Mainline DHT put operation completed
│ [2025-10-17T10:32:26.464Z] ✓ Stored envelope at ARID
```

Retrieval is transparent - Hybrid automatically determines the correct backend:

```
hubert get --storage hybrid $ARID_SMALL
hubert get --storage hybrid $ARID_LARGE

│ ur:envelope/tpsoihgujnhsjzjzsrrtsskg
│ ur:envelope/tpsohkattiecfzfdswresrtbvdwsetcyveguwdolgdvdamgsdnv...
```

### Using Hubert Server

The Hubert server provides centralized low-latency storage for testing, development, and controlled environments.

**Starting the server** (note: this command blocks, so run in separate terminal):

```
# Start server on default port (8080)
hubert server

Starting Hubert server on port 45678 with in-memory storage
✓ Hubert server listening on 127.0.0.1:45678
```

**Using the server** (in another terminal):

```
ARID=$(hubert generate arid)
ENVELOPE=$(envelope subject type string "Server message")
hubert put --storage server $ARID $ENVELOPE
hubert get --storage server $ARID

│ ur:envelope/tpsojtguihjpkoihjpcxjnihjkjkhsioihjpryisve
```

**Server-specific options**:

```
# Store with TTL (time-to-live) in seconds
ARID=$(hubert generate arid)
hubert put --storage server --ttl 3600 $ARID $ENVELOPE
```

## Advanced Usage

### Verbose Output

Enable verbose logging to see detailed operation information:

```
ARID=$(hubert generate arid)
hubert --storage server --verbose put $ARID $ENVELOPE

│ [2025-10-17T10:37:32.314Z] Starting server put operation
│ [2025-10-17T10:37:32.315Z] Sending PUT request to server
│ [2025-10-17T10:37:32.329Z] Server put operation completed
│ [2025-10-17T10:37:32.329Z] ✓ Stored envelope at ARID
```

### Timeouts

Control how long to wait for retrieval operations:

```
# Wait up to 60 seconds (default is 30)
hubert get --timeout 60 $ARID

│ ur:envelope/tpsojkihjkjyjljtihjkkshsjpjeihcxfefgfpam
```

If the timeout expires:

```
hubert get --timeout 5 $NONEXISTENT_ARID

│ Error: Timeout: Failed to retrieve envelope after 5 seconds
```

### IPFS Pinning

By default, IPFS content is not pinned and may be garbage collected. Use `--pin` to ensure persistence (as long as your IPFS node is running):

```
hubert put --storage ipfs --pin $ARID $ENVELOPE

│ ✓ Stored and pinned envelope at IPFS
│ CID: QmX7Zx3YyP8mN9oQ5rT6vW2pL4kJ8hF3gR1sD9eT5mC7nA
```

Pinned content remains available until explicitly unpinned:

```
ipfs pin ls QmX7Zx3YyP8mN9oQ5rT6vW2pL4kJ8hF3gR1sD9eT5mC7nA

│ QmX7Zx3YyP8mN9oQ5rT6vW2pL4kJ8hF3gR1sD9eT5mC7nA recursive
```

### Server TTL

When using the server backend, specify how long data should be retained:

```
# Store with 1 hour TTL
hubert put --storage server --ttl 3600 $ARID $ENVELOPE

│ ✓ Stored envelope with 3600s TTL
```

```
# Store with 24 hour TTL
hubert put --storage server --ttl 86400 $ARID $ENVELOPE

│ ✓ Stored envelope with 86400s TTL
```

After the TTL expires, the server automatically removes the data.

## Bidirectional Communication Pattern

Hubert enables request-response flows without direct connections between parties.

### Request-Response Flow

**Example scenario**: Alice wants Bob to sign a document.

**Step 1: Alice prepares the request**

```
# Alice generates ARID for the request and expected response
REQUEST_ARID=$(hubert generate arid)
RESPONSE_ARID=$(hubert generate arid)

# Alice creates an envelope with the document and response ARID
# (In practice, this would be encrypted with GSTP to Bob's public key)
REQUEST_ENVELOPE=$(envelope subject type string "Please sign: document.pdf" | \
  envelope assertion add pred-obj string "responseArid" string "$RESPONSE_ARID")
```

**Step 2: Alice publishes the request**

```
hubert put $REQUEST_ARID $REQUEST_ENVELOPE

│ ✓ Stored envelope at ur:arid/hdcx...
```

**Step 3: Alice shares REQUEST_ARID with Bob**

Alice sends the REQUEST_ARID to Bob via a secure channel (Signal, QR code, encrypted email, etc.). The ARID is never published to the storage network.

**Step 4: Bob retrieves the request**

```
# Bob receives REQUEST_ARID from Alice via secure channel
REQUEST_ARID="ur:arid/hdcx..."

# Bob retrieves the request
hubert get $REQUEST_ARID

│ ur:envelope/lftpsoihfpjzinjljljzinjpjljtjkjyihjzjpjtjnihjpjnisinjnjtihihihih...
```

```
# Bob extracts the response ARID from the request
RESPONSE_ARID=$(hubert get $REQUEST_ARID | \
  envelope assertion find pred string "responseArid" | \
  envelope extract string)

echo $RESPONSE_ARID

│ ur:arid/hdcxbwmwcwfdkecauerfvsdirpwpfhfgtalfmulesnstvlrpoyfzuyenamdpmdcfutdl
```

**Step 5: Bob creates and publishes the response**

```
# Bob creates his response (signature, etc.)
RESPONSE_ENVELOPE=$(envelope subject type string "Signed: document.pdf.sig")

# Bob publishes at the RESPONSE_ARID that Alice specified
hubert put $RESPONSE_ARID $RESPONSE_ENVELOPE

│ ✓ Stored envelope at ur:arid/hdcxbwmw...
```

**Step 6: Alice retrieves the response**

```
# Alice already knows the RESPONSE_ARID (she generated it)
hubert get $RESPONSE_ARID

│ ur:envelope/tpsojkisinjkinishsjtjlihjzinidhlroiehl
```

```
# Alice extracts Bob's signature
hubert get $RESPONSE_ARID | envelope extract string

│ Signed: document.pdf.sig
```

**Key points**:
- ARIDs never published to storage - shared only via secure channels
- No direct connection needed between Alice and Bob
- Parties don't need to be online simultaneously
- Storage network sees only encrypted envelopes (with GSTP)
- Write-once semantics prevent tampering

## Integration with GSTP

Hubert is designed to work with GSTP (Gordian Sealed Transaction Protocol) for end-to-end encryption. GSTP envelopes encrypt the subject and seal recipient assertions, providing complete opacity to storage networks.

**Example: Encrypted communication**

```
# Alice creates a GSTP envelope encrypted to Bob's public key
# (Using the 'envelope' CLI with GSTP support)
ALICE_REQUEST=$(envelope subject type string "Secret message" | \
  envelope encrypt recipient --recipient-pubkey $BOB_PUBKEY | \
  envelope sign --signer $ALICE_PRIVKEY)

# Alice stores the encrypted envelope
REQUEST_ARID=$(hubert generate arid)
hubert put $REQUEST_ARID $ALICE_REQUEST

│ ✓ Stored envelope at ur:arid/hdcx...
```

**What the storage network sees**: Only the encrypted GSTP envelope - subject is encrypted, recipient assertions are sealed. No plaintext, no metadata, no indication of content.

**What Bob sees**: After retrieving and decrypting with his private key, Bob sees the original message, verifies Alice's signature, and can extract the response ARID from the decrypted content.

This combination of Hubert's write-once storage and GSTP's encryption provides:
- **Network opacity**: Storage nodes see only ciphertext
- **Sender authentication**: Cryptographic signatures prove origin
- **Receiver privacy**: Only intended recipients can decrypt
- **Message integrity**: Write-once prevents tampering
- **Capability-based access**: ARID holder can read; ARID creator can write

## Troubleshooting

**Problem**: DHT operations are slow or failing

```
hubert --verbose put $ARID $ENVELOPE

│ [2025-10-17T14:45:23Z] Bootstrapping into DHT network...
│ [2025-10-17T14:45:33Z] Error: Failed to bootstrap: No peers available
```

**Solution**: DHT requires network connectivity and can take 10-30 seconds to bootstrap. Check your internet connection and firewall settings. UDP port 6881 must not be blocked.

---

**Problem**: IPFS operations failing

```
hubert check --storage ipfs

│ Error: IPFS daemon not available at http://127.0.0.1:5001
```

**Solution**: Start the IPFS daemon:

```
ipfs daemon

│ Initializing daemon...
│ API server listening on /ip4/127.0.0.1/tcp/5001
```

---

**Problem**: Write-once violation error

```
hubert put $ARID $ENVELOPE

│ Error: Write-once violation: ARID already exists
```

**Solution**: This is expected behavior - each ARID can only be written once. Generate a new ARID for new data:

```
NEW_ARID=$(hubert generate arid)
hubert put $NEW_ARID $ENVELOPE
```

---

**Problem**: Retrieval timeout

```
hubert get $ARID

│ Error: Timeout: Failed to retrieve envelope after 30 seconds
```

**Solutions**:
- Increase timeout: `hubert get --timeout 60 $ARID`
- For DHT: Data may not be sufficiently propagated yet (wait and retry)
- For IPFS: Content may not be pinned or provider offline
- Verify ARID was actually written: Check your put operation succeeded

---

**Problem**: Invalid ARID or envelope format

```
hubert put "not-a-valid-arid" $ENVELOPE

│ Error: Invalid ARID format: expected ur:arid/...
```

**Solution**: ARIDs and envelopes must be in UR format:
- ARIDs: `ur:arid/hdcx...`
- Envelopes: `ur:envelope/tpsoi...`

Generate valid identifiers:

```
ARID=$(hubert generate arid)
ENVELOPE=$(envelope subject type string "data")
```

---

**Problem**: Message too large for DHT

```
hubert put $ARID $LARGE_ENVELOPE

│ Error: Envelope too large for DHT storage (1523 bytes, max 1000 bytes)
```

**Solution**: Use IPFS or Hybrid storage for large messages:

```
hubert put --storage ipfs $ARID $LARGE_ENVELOPE
# or
hubert put --storage hybrid $ARID $LARGE_ENVELOPE
```

## Command Reference

**Global Options** (apply to all commands):

```
-s, --storage <STORAGE>    Storage backend: mainline, ipfs, hybrid, server [default: mainline]
    --host <HOST>          Server/IPFS host
    --port <PORT>          Port for server/IPFS/hybrid
-v, --verbose              Enable verbose logging
-h, --help                 Print help
-V, --version              Print version
```

---

**`hubert generate arid`** - Generate a new ARID

```
hubert generate arid

│ ur:arid/hdcxuestvsdemusrdlkngwtosweortdwbasrdrfxhssgfmvlrflthdplatjydmmwahgdwlflguqz
```

Output: A new ARID in UR format

---

**`hubert generate envelope`** - Generate a test envelope

```
hubert generate envelope

│ ur:envelope/tpsoiyfdihjzjzjldmksbaoede
```

Output: A test envelope with random data in UR format

---

**`hubert put`** - Store an envelope at an ARID

```
hubert put [OPTIONS] <ARID> <ENVELOPE>
```

Arguments:
- `<ARID>`: ARID key in `ur:arid` format
- `<ENVELOPE>`: Envelope value in `ur:envelope` format

Options:
- `--ttl <TTL>`: Time-to-live in seconds (server backend only)
- `--pin`: Pin content in IPFS (ipfs/hybrid backend only)

Example:

```
hubert put ur:arid/hdcx... ur:envelope/tpsoi...

│ ✓ Stored envelope at ur:arid/hdcx...
```

---

**`hubert get`** - Retrieve an envelope by ARID

```
hubert get [OPTIONS] <ARID>
```

Arguments:
- `<ARID>`: ARID key in `ur:arid` format

Options:
- `-t, --timeout <TIMEOUT>`: Maximum time to wait in seconds [default: 30]

Example:

```
hubert get ur:arid/hdcx...

│ ur:envelope/tpsoi...
```

---

**`hubert check`** - Check if storage backend is available

```
hubert check [OPTIONS]
```

Options: (uses global --storage, --host, --port options)

Example:

```
hubert check --storage ipfs

│ ✓ IPFS is available at http://127.0.0.1:5001
```

---

**`hubert server`** - Start the Hubert HTTP server

```
hubert server [OPTIONS]
```

Options:
- `--port <PORT>`: Port to listen on [default: 8080]

Example:

```
hubert server --port 8080

│ 🚀 Hubert server starting...
│ 📦 Storage: SQLite database at hubert.sqlite
│ 🌐 Listening on http://0.0.0.0:8080
```

**Note**: This command blocks and runs the server. Use Ctrl+C to stop.
