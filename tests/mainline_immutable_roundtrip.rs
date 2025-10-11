use anyhow::Result;
use mainline::{Dht, Testnet};
use tokio::time::{Duration, sleep};

/// Configuration for DHT test scenarios.
enum DhtConfig {
    /// In-process testnet with specified number of bootstrap nodes.
    Testnet(usize),
    /// Real Mainline DHT using default bootstrap nodes.
    Mainnet,
}

/// Shared test logic: stores an immutable value via one AsyncDHT node,
/// then retrieves it from a separate AsyncDHT node after a short delay.
async fn immutable_roundtrip_test(config: DhtConfig) -> Result<()> {
    // Keep testnet alive for the entire test duration.
    let _testnet;
    let (writer, reader, timeout) = match config {
        DhtConfig::Testnet(nodes) => {
            // Create an in-process DHT with its own bootstrap nodes.
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
            // Two separate agents on mainnet using default bootstrap nodes.
            let writer = Dht::client()?.as_async();
            let reader = Dht::client()?.as_async();

            (writer, reader, Duration::from_secs(30))
        }
    };

    // Ensure both nodes are bootstrapped before proceeding.
    assert!(writer.bootstrapped().await, "writer failed to bootstrap");
    assert!(reader.bootstrapped().await, "reader failed to bootstrap");

    // Value must be small; BEP-44 values are ~<= 1 KiB after bencode.
    let msg = b"hello from mainline";

    // Writer stores immutable value (key = SHA-1(msg)); returns the lookup Id.
    let id = writer.put_immutable(msg).await?;

    // Give the network a moment to replicate.
    let delay = match config {
        DhtConfig::Testnet(_) => Duration::from_millis(200),
        DhtConfig::Mainnet => Duration::from_secs(1),
    };
    sleep(delay).await;

    // Reader polls until it sees the value (with a soft timeout).
    let deadline = tokio::time::Instant::now() + timeout;
    let mut got = None;

    loop {
        if let Some(bytes) = reader.get_immutable(id).await {
            got = Some(bytes);
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        let poll_interval = match config {
            DhtConfig::Testnet(_) => Duration::from_millis(100),
            DhtConfig::Mainnet => Duration::from_millis(250),
        };
        sleep(poll_interval).await;
    }

    let bytes = got.expect("reader did not retrieve the value in time");
    assert_eq!(&*bytes, msg, "round-tripped value must match original");

    Ok(())
}

/// Testnet variant: runs against an in-process DHT.
#[tokio::test(flavor = "multi_thread")]
async fn immutable_put_then_get_testnet() -> Result<()> {
    immutable_roundtrip_test(DhtConfig::Testnet(5)).await
}

/// Mainnet variant: hits the real Mainline DHT. Requires outbound UDP.
/// Run with: cargo test -q -- --ignored --nocapture immutable_put_then_get_mainnet
#[tokio::test(flavor = "multi_thread")]
#[ignore = "hits the real Mainline DHT and needs UDP connectivity"]
async fn immutable_put_then_get_mainnet() -> Result<()> {
    immutable_roundtrip_test(DhtConfig::Mainnet).await
}
