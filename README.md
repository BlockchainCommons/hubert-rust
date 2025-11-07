# Blockchain Commons Hubert

<!--Guidelines: https://github.com/BlockchainCommons/secure-template/wiki -->

### _by [Wolf McNally](https://www.github.com/wolfmcnally) and [Christopher Allen](https://www.github.com/ChristopherA)_

---

## Introduction

**Hubert** provides a distributed infrastructure for secure multiparty transactions, such as FROST threshold signature ceremonies, enabling participants to communicate bidirectionally with complete opacity to outsiders. By leveraging write-once distributed storage with cryptographic identifiers, Hubert creates a trustless coordination layer where parties can exchange encrypted messages without relying on centralized servers or exposing sensitive information to network observers.

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

Ted Nelson's Project Xanadu had its own playful jargon. The basic object that behaved like a "file" was called a **bert**—named after **Bertrand Russell**. And because geeks can't resist wordplay, there was also an **ernie**, the metered unit of billing in the publishing system.

Mark S. Miller, one of Xanadu's architects, later designed the **Club System** (early groundwork for his capability-security thinking), which modeled group permissions but still relied on identity-checked ACLs rather than pure capabilities. That historical thread matters because Hubert sits exactly where Xanadu's ideas were pointing, but finishes the job with cryptography.

So: **Hubert** is the **hub of berts**. In Xanadu terms, it's the rendezvous point where these file-like objects (and their successors) can meet, exchange sealed messages, and coordinate—without servers, accounts, or trusted intermediaries. It's a deliberate nod to Nelson's vocabulary and to the "clubs" lineage, reframed for an era where capability comes from math, not administrators.

There's also a second layer to the name. Cryptography uses a stock cast—**Alice**, **Bob**, **Carol**, et al.—to illustrate protocols. **Hubert** joins that dramatis personae as the sturdy switchboard operator in the background: the dropbox, dead-drop, and message hub that keeps multiparty ceremonies moving while revealing nothing but ciphertext to the outside world.

## Getting Started

### As a Command-Line Tool

Install `hubert` from crates.io:

```bash
cargo install hubert
```

Or install from source:

```bash
cd hubert
cargo install --path .
```

See the [CLI Manual](./docs/CLIManual.md) for detailed usage instructions.

### As a Rust Library

Add Hubert to your `Cargo.toml`:

```toml
[dependencies]
hubert = "0.2.0"
bc-components = "^0.25.0"
bc-envelope = "^0.34.0"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

See the [API Manual](./docs/APIManual.md) for detailed usage instructions.


## Key Capabilities

### 1. Write-Once Distributed Storage

Hubert provides APIs for four storage backends, all using write-once semantics:

- **BitTorrent Mainline DHT**: Fast, lightweight, serverless (≤1 KB messages)
- **IPFS**: Large capacity, content-addressed (up to 10 MB messages)
- **Hybrid**: Automatic optimization by size, combining DHT speed with IPFS capacity
- **Server**: Centralized storage for testing and controlled deployments (configurable size limits)

The first three backends (DHT, IPFS, Hybrid) provide decentralized, trustless operation suitable for production use. The Server backend is designed for development, testing, and controlled environments where centralized coordination is acceptable.

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

Hubert's storage layer is designed to work with GSTP-encrypted payloads, ensuring end-to-end security for multiparty transactions.

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
- **Large messages (>1 KB)**: Reference stored in DHT (encrypted to hide IPFS ARID), actual content in IPFS
- **Transparent indirection**: Applications use same API regardless of size
- **Reference encryption**: IPFS ARIDs hidden from DHT observers using ARID-derived encryption keys

This enables applications to send compact control messages via DHT while supporting large payloads (key material, proofs, documents) via IPFS without changing code. The reference envelope encryption is an internal security measure independent of application-layer GSTP encryption.

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

## Version History

### 0.2.0 - November 7, 2025

- Encrypt hybrid layer reference envelopes to hide IPFS ARIDs from DHT observers.
- Add ARID-derived encryption keys for reference envelope security.
- Update documentation to reflect reference encryption behavior.
- Fix tests.
- Fix repo name.

### 0.1.0 - October 18, 2025

- Initial release.

## Status - Community Review

Hubert is currently in the community review phase and should not be used for production tasks until it has had further testing and auditing.

See [Blockchain Commons' Development Phases](https://github.com/BlockchainCommons/Community/blob/master/release-path.md).

## Financial Support

Hubert is a project of [Blockchain Commons](https://www.blockchaincommons.com/). We are proudly a "not-for-profit" social benefit corporation committed to open source & open development. Our work is funded entirely by donations and collaborative partnerships with people like you. Every contribution will be spent on building open tools, technologies, and techniques that sustain and advance blockchain and internet security infrastructure and promote an open web.

To financially support further development of Hubert and other projects, please consider becoming a Patron of Blockchain Commons through ongoing monthly patronage as a [GitHub Sponsor](https://github.com/sponsors/BlockchainCommons). You can also support Blockchain Commons with bitcoins at our [BTCPay Server](https://btcpay.blockchaincommons.com/).

## Contributing

We encourage public contributions through issues and pull requests! Please review [CONTRIBUTING.md](./CONTRIBUTING.md) for details on our development process. All contributions to this repository require a GPG signed [Contributor License Agreement](./CLA.md).

### Discussions

The best place to talk about Blockchain Commons and its projects is in our GitHub Discussions areas.

[**Gordian Developer Community**](https://github.com/BlockchainCommons/Gordian-Developer-Community/discussions). For standards and open-source developers who want to talk about interoperable wallet specifications, please use the Discussions area of the [Gordian Developer Community repo](https://github.com/BlockchainCommons/Gordian-Developer-Community/discussions). This is where you talk about Gordian specifications such as [Gordian Envelope](https://github.com/BlockchainCommons/Gordian/tree/master/Envelope#articles), [bc-shamir](https://github.com/BlockchainCommons/bc-shamir), [Sharded Secret Key Reconstruction](https://github.com/BlockchainCommons/bc-sskr), and [bc-ur](https://github.com/BlockchainCommons/bc-ur) as well as the larger [Gordian Architecture](https://github.com/BlockchainCommons/Gordian/blob/master/Docs/Overview-Architecture.md), its [Principles](https://github.com/BlockchainCommons/Gordian#gordian-principles) of independence, privacy, resilience, and openness, and its macro-architectural ideas such as functional partition (including airgapping, the original name of this community).

[**Gordian User Community**](https://github.com/BlockchainCommons/Gordian/discussions). For users of the Gordian reference apps, including [Gordian Coordinator](https://github.com/BlockchainCommons/iOS-GordianCoordinator), [Gordian Seed Tool](https://github.com/BlockchainCommons/GordianSeedTool-iOS), [Gordian Server](https://github.com/BlockchainCommons/GordianServer-macOS), [Gordian Wallet](https://github.com/BlockchainCommons/GordianWallet-iOS), and [SpotBit](https://github.com/BlockchainCommons/spotbit) as well as our whole series of [CLI apps](https://github.com/BlockchainCommons/Gordian/blob/master/Docs/Overview-Apps.md#cli-apps). This is a place to talk about bug reports and feature requests as well as to explore how our reference apps embody the [Gordian Principles](https://github.com/BlockchainCommons/Gordian#gordian-principles).

[**Blockchain Commons Discussions**](https://github.com/BlockchainCommons/Community/discussions). For developers, interns, and patrons of Blockchain Commons, please use the discussions area of the [Community repo](https://github.com/BlockchainCommons/Community) to talk about general Blockchain Commons issues, the intern program, or topics other than those covered by the [Gordian Developer Community](https://github.com/BlockchainCommons/Gordian-Developer-Community/discussions) or the
[Gordian User Community](https://github.com/BlockchainCommons/Gordian/discussions).

### Other Questions & Problems

As an open-source, open-development community, Blockchain Commons does not have the resources to provide direct support of our projects. Please consider the discussions area as a locale where you might get answers to questions. Alternatively, please use this repository's [issues](./issues) feature. Unfortunately, we can not make any promises on response time.

If your company requires support to use our projects, please feel free to contact us directly about options. We may be able to offer you a contract for support from one of our contributors, or we might be able to point you to another entity who can offer the contractual support that you need.

### Credits

The following people directly contributed to this repository. You can add your name here by getting involved. The first step is learning how to contribute from our [CONTRIBUTING.md](./CONTRIBUTING.md) documentation.

| Name              | Role                     | Github                                           | Email                                 | GPG Fingerprint                                    |
| ----------------- | ------------------------ | ------------------------------------------------ | ------------------------------------- | -------------------------------------------------- |
| Christopher Allen | Principal Architect      | [@ChristopherA](https://github.com/ChristopherA) | \<ChristopherA@LifeWithAlacrity.com\> | FDFE 14A5 4ECB 30FC 5D22  74EF F8D3 6C91 3574 05ED |
| Wolf McNally      | Lead Researcher/Engineer | [@WolfMcNally](https://github.com/wolfmcnally)   | \<Wolf@WolfMcNally.com\>              | 9436 52EE 3844 1760 C3DC  3536 4B6C 2FCF 8947 80AE |

## Responsible Disclosure

We want to keep all of our software safe for everyone. If you have discovered a security vulnerability, we appreciate your help in disclosing it to us in a responsible manner. We are unfortunately not able to offer bug bounties at this time.

We do ask that you offer us good faith and use best efforts not to leak information or harm any user, their data, or our developer community. Please give us a reasonable amount of time to fix the issue before you publish it. Do not defraud our users or us in the process of discovery. We promise not to bring legal action against researchers who point out a problem provided they do their best to follow the these guidelines.

### Reporting a Vulnerability

Please report suspected security vulnerabilities in private via email to ChristopherA@BlockchainCommons.com (do not use this email for support). Please do NOT create publicly viewable issues for suspected security vulnerabilities.

The following keys may be used to communicate sensitive information to developers:

| Name              | Fingerprint                                        |
| ----------------- | -------------------------------------------------- |
| Christopher Allen | FDFE 14A5 4ECB 30FC 5D22  74EF F8D3 6C91 3574 05ED |

You can import a key by running the following command with that individual's fingerprint: `gpg --recv-keys "<fingerprint>"` Ensure that you put quotes around fingerprints that contain spaces.

## Related Projects

- **[Gordian Envelope](https://github.com/BlockchainCommons/bc-envelope-rust)**: Structured data format with encryption and signing
- **[GSTP](https://github.com/BlockchainCommons/gstp-rust)**: Sealed transaction protocol for secure messaging
- **[Clubs](https://github.com/BlockchainCommons/clubs-rust)**: Gordian Clubs
- **[bc-components](https://github.com/BlockchainCommons/bc-components-rust)**: Cryptographic components including ARID

---

**Hubert**: Enabling trustless coordination for secure multiparty transactions.
