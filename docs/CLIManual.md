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

Install from crates.io:

```bash
cargo install hubert
```

Or build and install from source:

```bash
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
│   generate  Generate a new ARID or example Envelope
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

│ ur:arid/hdcxiedwnbzooxpdihcmfykpvodagrmdjsidrespnbjeemfdgmdnesgeiaeocxprftbboxcsfsks
```

### Creating an Envelope

For testing, you can generate a test envelope with random data:

```
hubert generate envelope 20 # Number of random bytes

│ ur:envelope/tpsoghdldkjswyksidgadskggdsaflrfvlylrpzoseetiolkutdmlf
```

Or create a real envelope using the `envelope` CLI tool (from bc-envelope-cli):

```
ENVELOPE=$(envelope subject type string 'Hello, Hubert')
echo $ENVELOPE
envelope format $ENVELOPE

│ ur:envelope/tpsojnfdihjzjzjldwcxfdkpidihjpjyoynyghtd
│ "Hello, Hubert"
```

### Storing Data (Put)

Store an envelope at an ARID using the default storage backend (Mainline DHT). No output indicates success:

```
hubert put $ARID $ENVELOPE
```

**Important**: Each ARID can only be written once. Attempting to write again will fail:

```
hubert put $ARID $ENVELOPE

│ Error: ur:arid/hdcxiedwnbzooxpdihcmfykpvodagrmdjsidrespnbjeemfdgmdnesgeiaeocxprftbboxcsfsks already exists
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

│ Error: ✗ Server is not available at 127.0.0.1:1234: error sending request for url (http://127.0.0.1:1234/health)
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

│ ur:envelope/tpsojefyfdghcxjnihjkjkhsioihmusnlpsp
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

│ ur:envelope/tpsojzgagdfggucxjnihjkjkhsioihzmeslohk
```

**IPFS Pinning**: By default, content is not pinned. Use `--pin` to ensure persistence. This example uses the `--verbose` flag to show detailed output:

```
ARID=$(hubert generate arid)
hubert put --storage ipfs --pin --verbose $ARID $ENVELOPE

│ [2025-10-18T10:02:03.272Z] Starting IPFS put operation
│ [2025-10-18T10:02:03.273Z] Envelope size: 17 bytes
│ [2025-10-18T10:02:03.273Z] Getting or creating IPNS key
│ [2025-10-18T10:02:03.325Z] Adding content to IPFS
│ [2025-10-18T10:02:03.366Z] Content CID: QmPUKGjPUbJQVb5PonRh21EtaNJ19UHXUNqoFmSovT4YpT
│ [2025-10-18T10:02:03.366Z] Pinning content
│ [2025-10-18T10:02:03.396Z] Publishing to IPNS (write-once check)
│ [2025-10-18T10:02:38.998Z] IPFS put operation completed
│ [2025-10-18T10:02:38.998Z] ✓ Stored envelope at ARID
│ CID: QmPUKGjPUbJQVb5PonRh21EtaNJ19UHXUNqoFmSovT4YpT
```

See pinned content:

```
ipfs pin ls

│ QmPUKGjPUbJQVb5PonRh21EtaNJ19UHXUNqoFmSovT4YpT recursive
```

Unpin the content:

```
ipfs pin rm QmPUKGjPUbJQVb5PonRh21EtaNJ19UHXUNqoFmSovT4YpT

│ unpinned QmPUKGjPUbJQVb5PonRh21EtaNJ19UHXUNqoFmSovT4YpT
```

### Using Hybrid Storage

Hybrid mode automatically optimizes storage based on message size:

```
# Small message (≤1 KB) - uses DHT
SMALL_MSG=$(envelope subject type string "Small")
ARID_SMALL=$(hubert generate arid)
hubert put --storage hybrid --verbose $ARID_SMALL $SMALL_MSG

│ [2025-10-18T10:08:31.138Z] Storing envelope in DHT (size ≤ 1000 bytes)
│ [2025-10-18T10:08:31.138Z] Starting Mainline DHT put operation
│ [2025-10-18T10:08:31.138Z] Envelope size: 10 bytes
│ [2025-10-18T10:08:31.138Z] Deriving DHT signing key from ARID
│ [2025-10-18T10:08:31.139Z] Checking for existing value (write-once check)
│ [2025-10-18T10:08:34.174Z] Creating mutable DHT item
│ [2025-10-18T10:08:34.175Z] Putting value to DHT
│ [2025-10-18T10:08:36.339Z] Mainline DHT put operation completed
│ [2025-10-18T10:08:36.339Z] ✓ Stored envelope at ARID
```

```
# Large message (>1 KB) - uses IPFS with DHT reference
LARGE_MSG=$(hubert generate envelope 2000) # 2000 random bytes
ARID_LARGE=$(hubert generate arid)
hubert put --storage hybrid --verbose $ARID_LARGE $LARGE_MSG

│ [2025-10-18T10:08:57.140Z] Envelope too large for DHT, using IPFS indirection
│ [2025-10-18T10:08:57.140Z] Storing actual envelope in IPFS with reference ARID: ur:arid/hdcxgskpendlfrlesrcssbbkrtmewnzmrdbyeorsvstpdyfhcnhfmklessrelettldgalfjtcmny
│ [2025-10-18T10:08:57.140Z] Starting IPFS put operation
│ [2025-10-18T10:08:57.140Z] Envelope size: 2007 bytes
│ [2025-10-18T10:08:57.140Z] Getting or creating IPNS key
│ [2025-10-18T10:08:57.208Z] Adding content to IPFS
│ [2025-10-18T10:08:57.243Z] Content CID: QmZmKaSWuowBPsWEEp5JsZ7tkPwmjhAtyEpvqrvKhsNN3a
│ [2025-10-18T10:08:57.243Z] Publishing to IPNS (write-once check)
│ [2025-10-18T10:09:28.157Z] IPFS put operation completed
│ [2025-10-18T10:09:28.160Z] Storing reference envelope in DHT at original ARID
│ [2025-10-18T10:09:28.160Z] Starting Mainline DHT put operation
│ [2025-10-18T10:09:28.161Z] Envelope size: 72 bytes
│ [2025-10-18T10:09:28.161Z] Deriving DHT signing key from ARID
│ [2025-10-18T10:09:28.161Z] Checking for existing value (write-once check)
│ [2025-10-18T10:09:30.978Z] Creating mutable DHT item
│ [2025-10-18T10:09:30.978Z] Putting value to DHT
│ [2025-10-18T10:09:32.524Z] Mainline DHT put operation completed
│ [2025-10-18T10:09:32.524Z] ✓ Stored envelope at ARID
```

Retrieval is transparent - Hybrid automatically determines the correct backend:

```
hubert get --storage hybrid $ARID_SMALL
hubert get --storage hybrid $ARID_LARGE

│ ur:envelope/tpsoihgujnhsjzjzsrrtsskg
│ ur:envelope/tpsohkattifzfppdlrrhvybnflhdjoptmtzshtwfotpdfltkgreerylddsotnlkknlsooy...
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

│ [2025-10-18T10:11:26.160Z] Starting server put operation
│ [2025-10-18T10:11:26.161Z] Sending PUT request to server
│ [2025-10-18T10:11:26.166Z] Server put operation completed
│ [2025-10-18T10:11:26.166Z] ✓ Stored envelope at ARID
```

### Timeouts

Control how long to wait for retrieval operations:

```
# Wait up to 60 seconds (default is 30)
hubert get --timeout 60 $ARID

│ ur:envelope/tpsojtguihjpkoihjpcxjnihjkjkhsioihjpryisve
```

If the timeout expires:

```
NONEXISTENT_ARID=$(hubert generate arid)
hubert get --timeout 5 $NONEXISTENT_ARID

│ Error: Value not found within 5 seconds
```

### IPFS Pinning

By default, IPFS content is not pinned and may be garbage collected. Use `--pin` to ensure persistence (as long as your IPFS node is running).
The command's output shows the pinned CID:

```
ARID=$(hubert generate arid)
ENVELOPE=$(envelope subject type string "Pinned IPFS message")
hubert put --storage ipfs --pin $ARID $ENVELOPE

│ CID: QmZWpMdDR1Y1zWCziJByWFs6rRFZ8zXRCxuh9dbhg5u9BR
```

Pinned content remains available until explicitly unpinned:

```
ipfs pin ls QmZWpMdDR1Y1zWCziJByWFs6rRFZ8zXRCxuh9dbhg5u9BR

│ QmZWpMdDR1Y1zWCziJByWFs6rRFZ8zXRCxuh9dbhg5u9BR recursive
```

The returned CID can be used to unpin later if desired.

```
ipfs pin rm QmZWpMdDR1Y1zWCziJByWFs6rRFZ8zXRCxuh9dbhg5u9BR
ipfs pin ls QmZWpMdDR1Y1zWCziJByWFs6rRFZ8zXRCxuh9dbhg5u9BR

│ Error: path 'QmZWpMdDR1Y1zWCziJByWFs6rRFZ8zXRCxuh9dbhg5u9BR' is not pinned
```

### Server TTL

When using the server backend, specify how long data should be retained:

```
# Store with 1 hour TTL
ARID=$(hubert generate arid)
ENVELOPE=$(envelope subject type string "Temporary message")
hubert put --storage server --ttl 3600 $ARID $ENVELOPE
```

```
# Store with 24 hour TTL
ARID=$(hubert generate arid)
ENVELOPE=$(envelope subject type string "One day message")
hubert put --storage server --ttl 86400 $ARID $ENVELOPE
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
  envelope assertion add pred-obj string "responseArid" arid "$RESPONSE_ARID")

echo $REQUEST_ARID
echo $RESPONSE_ARID
echo $REQUEST_ENVELOPE
envelope format $REQUEST_ENVELOPE

│ ur:arid/hdcxrdbybkbbkbchcfknykdispmkghiacahgecishyetwnvestpsttwepkhtzeknttveswvozsgu
│ ur:arid/hdcxfzditpftaeonvosnbslteykgpkptfrmeguntbdimoytlfmmncypeckvdylpyhfesmyethdbd
│ ur:envelope/lftpsokscfgdjzihhsjkihcxjkiniojtftcxiejliakpjnihjtjydmjoieiyoytpsojzjpihjkjojljtjkihfpjpinietpsotansgshdcxfzditpftaeonvosnbslteykgpkptfrmeguntbdimoytlfmmncypeckvdylpyhfesbzcaqdbk
│ "Please sign: document.pdf" [
│     "responseArid": ARID(4027d83a)
│ ]
```

**Step 2: Alice publishes the request**

```
hubert put $REQUEST_ARID $REQUEST_ENVELOPE
```

**Step 3: Alice shares REQUEST_ARID with Bob**

Alice sends the REQUEST_ARID to Bob via a secure channel (Signal, QR code, encrypted email, etc.). The ARID is never published to the storage network.

**Step 4: Bob retrieves the request**

```
# Bob receives REQUEST_ARID from Alice via secure channel
RECEIVED_REQUEST_ARID=$REQUEST_ARID

# Bob retrieves the request
RECEIVED_REQUEST_ENVELOPE=$(hubert get $RECEIVED_REQUEST_ARID)
envelope format $RECEIVED_REQUEST_ENVELOPE

│ "Please sign: document.pdf" [
│     "responseArid": ARID(4027d83a)
│ ]
```

```
RECEIVED_RESPONSE_ARID=$( \
  envelope assertion find predicate string "responseArid" $RECEIVED_REQUEST_ENVELOPE | \
  envelope extract object | \
  envelope extract arid \
)

echo $RECEIVED_RESPONSE_ARID

│ ur:arid/hdcxfzditpftaeonvosnbslteykgpkptfrmeguntbdimoytlfmmncypeckvdylpyhfesmyethdbd
```

**Step 5: Bob creates and publishes the response**

```
# Bob creates his response (signature, etc.)
RESPONSE_ENVELOPE=$(envelope subject type string "Signed: document.pdf.sig")
envelope format $RESPONSE_ENVELOPE

│ "Signed: document.pdf.sig"
```

```
# Bob publishes at the RESPONSE_ARID that Alice specified
hubert put $RECEIVED_RESPONSE_ARID $RESPONSE_ENVELOPE
```

**Step 6: Alice retrieves the response**

```
# Alice already knows the RESPONSE_ARID (she generated it)
RECEIVED_RESPONSE_ENVELOPE=$(hubert get $RESPONSE_ARID)
envelope extract string $RECEIVED_RESPONSE_ENVELOPE

│ Signed: document.pdf.sig
```

**Key points**:
- ARIDs shared only via secure channels
- No direct connection needed between Alice and Bob
- Parties don't need to be online simultaneously
- Storage network sees only encrypted envelopes
- Write-once semantics prevent tampering
- Encryption and authentication using GSTP ensures confidentiality and integrity
