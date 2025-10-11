use anyhow::Result;
use mainline::{Dht, MutableItem, SigningKey, Testnet};
use tokio::time::{sleep, Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

/// Configuration for DHT test scenarios.
enum DhtConfig {
    /// In-process testnet with specified number of bootstrap nodes.
    Testnet(usize),
    /// Real Mainline DHT using default bootstrap nodes.
    Mainnet,
}

/// Shared test logic: stores a BEP-44 mutable value with a selected key (+salt),
/// then retrieves the most recent item from a separate agent.
async fn mutable_roundtrip_test(config: DhtConfig) -> Result<()> {
    // Keep testnet alive for the entire test duration.
    let _testnet;

    // Writer / Reader setup + timeout per environment.
    let (writer, reader, timeout) = match config {
        DhtConfig::Testnet(nodes) => {
            _testnet = Some(Testnet::new_async(nodes).await?);

            let writer = Dht::builder()
                .bootstrap(&_testnet.as_ref().unwrap().bootstrap)
                .build()?
                .as_async();

            let reader = Dht::builder()
                .bootstrap(&_testnet.as_ref().unwrap().bootstrap)
                .build()?
                .as_async();

            (writer, reader, Duration::from_secs(5))
        }
        DhtConfig::Mainnet => {
            _testnet = None;

            // Default bootstrap nodes (public mainnet).
            let writer = Dht::client()?.as_async();
            let reader = Dht::client()?.as_async();

            (writer, reader, Duration::from_secs(30))
        }
    };

    // Ensure both nodes are bootstrapped.
    assert!(writer.bootstrapped().await, "writer failed to bootstrap");
    assert!(reader.bootstrapped().await, "reader failed to bootstrap");

    // --- Authoring a mutable item with a selected key + fresh salt ---
    // Selected (deterministic) ed25519 key for the channel.
    // For real use, generate securely; for tests, determinism is fine.
    let signing_key = SigningKey::from_bytes(&[7u8; 32]);
    let pubkey = signing_key.verifying_key().to_bytes();

    // Use a unique salt per test run to avoid seq/collision on mainnet.
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock ok")
        .as_millis();
    let salt_buf = format!("ml-mutable-test-{now_ms}").into_bytes();
    let salt_opt: Option<&[u8]> = Some(salt_buf.as_slice());

    // The value to store (keep it small; BEP-44 values are ~≤1 KiB).
    let value: &[u8] = b"hello from mainline (mutable)";

    // Read-most-recent → compute next seq → CAS per BEP-44 guidance.
    // (See crate docs example for lost-update avoidance.) :contentReference[oaicite:1]{index=1}
    let (item, cas) = if let Some(mr) = writer.get_mutable_most_recent(&pubkey, salt_opt).await {
        let new_seq = mr.seq() + 1;
        (
            MutableItem::new(signing_key, value, new_seq, salt_opt),
            Some(mr.seq()),
        )
    } else {
        (MutableItem::new(signing_key, value, 1, salt_opt), None)
    };

    // Put the mutable item (signed under our selected key).
    writer.put_mutable(item, cas).await?; // returns Id; we don't need it here. :contentReference[oaicite:2]{index=2}

    // Allow replication.
    let delay = match config {
        DhtConfig::Testnet(_) => Duration::from_millis(200),
        DhtConfig::Mainnet => Duration::from_secs(1),
    };
    sleep(delay).await;

    // Reader polls the most-recent until it observes our value (soft deadline).
    let deadline = Instant::now() + timeout;
    let poll_interval = match config {
        DhtConfig::Testnet(_) => Duration::from_millis(100),
        DhtConfig::Mainnet => Duration::from_millis(250),
    };

    let mut got = None;
    loop {
        if let Some(mr) = reader.get_mutable_most_recent(&pubkey, salt_opt).await {
            got = Some(mr);
            break;
        }
        if Instant::now() >= deadline {
            break;
        }
        sleep(poll_interval).await;
    }

    let observed = got.expect("reader did not observe the mutable item in time");
    assert_eq!(observed.value(), value, "mutable value mismatch");
    // Optional sanity: check we fetched the right channel (pubkey echoed back).
    assert_eq!(observed.key(), &pubkey);

    Ok(())
}

/// Testnet variant: runs against an in-process DHT.
#[tokio::test(flavor = "multi_thread")]
async fn mutable_put_then_get_testnet() -> Result<()> {
    mutable_roundtrip_test(DhtConfig::Testnet(5)).await
}

/// Mainnet variant: hits the real Mainline DHT. Requires outbound UDP.
/// Run with: cargo test -q -- --ignored --nocapture mutable_put_then_get_mainnet
#[tokio::test(flavor = "multi_thread")]
#[ignore = "hits the real Mainline DHT and needs UDP connectivity"]
async fn mutable_put_then_get_mainnet() -> Result<()> {
    mutable_roundtrip_test(DhtConfig::Mainnet).await
}
