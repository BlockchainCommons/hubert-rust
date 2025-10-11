use anyhow::Result;
use mainline::{Dht, Testnet, async_dht::AsyncDht};
use tokio::time::{Duration, sleep};

/// Shared test logic: stores an immutable value via one AsyncDHT node,
/// then retrieves it from a separate AsyncDHT node after a short delay.
async fn immutable_roundtrip_test(
    writer: AsyncDht,
    reader: AsyncDht,
    timeout: Duration,
    delay: Duration,
    poll_interval: Duration,
) -> Result<()> {
    // Ensure both nodes are bootstrapped before proceeding.
    assert!(writer.bootstrapped().await, "writer failed to bootstrap");
    assert!(reader.bootstrapped().await, "reader failed to bootstrap");

    // Value must be small; BEP-44 values are ~<= 1 KiB after bencode.
    let msg = b"hello from mainline";

    // Writer stores immutable value (key = SHA-1(msg)); returns the lookup Id.
    let id = writer.put_immutable(msg).await?;

    // Give the network a moment to replicate.
    sleep(delay).await;

    // Reader polls until it sees the value (with a soft timeout).
    let start = tokio::time::Instant::now();
    let deadline = start + timeout;
    let mut got = None;
    let mut iterations = 0;

    loop {
        iterations += 1;
        if let Some(bytes) = reader.get_immutable(id).await {
            got = Some(bytes);
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        sleep(poll_interval).await;
    }

    let elapsed = start.elapsed();
    println!(
        "Poll stats: {} iterations, interval={:.3}s, total={:.3}s",
        iterations,
        poll_interval.as_secs_f64(),
        elapsed.as_secs_f64()
    );

    let bytes = got.expect("reader did not retrieve the value in time");
    assert_eq!(&*bytes, msg, "round-tripped value must match original");

    Ok(())
}

/// Testnet variant: runs against an in-process DHT.
#[tokio::test(flavor = "multi_thread")]
async fn immutable_put_then_get_testnet() -> Result<()> {
    // Create an in-process DHT with its own bootstrap nodes.
    let testnet = Testnet::new_async(5).await?;

    let writer = Dht::builder()
        .bootstrap(&testnet.bootstrap)
        .build()?
        .as_async();

    let reader = Dht::builder()
        .bootstrap(&testnet.bootstrap)
        .build()?
        .as_async();

    immutable_roundtrip_test(
        writer,
        reader,
        Duration::from_secs(5),
        Duration::from_millis(200),
        Duration::from_millis(100),
    )
    .await
}

/// Mainnet variant: hits the real Mainline DHT. Requires outbound UDP.
/// Run with: cargo test -q -- --ignored --nocapture immutable_put_then_get_mainnet
#[tokio::test(flavor = "multi_thread")]
#[ignore = "hits the real Mainline DHT and needs UDP connectivity"]
async fn immutable_put_then_get_mainnet() -> Result<()> {
    // Two separate agents on mainnet using default bootstrap nodes.
    let writer = Dht::client()?.as_async();
    let reader = Dht::client()?.as_async();

    immutable_roundtrip_test(
        writer,
        reader,
        Duration::from_secs(30),
        Duration::from_secs(1),
        Duration::from_millis(250),
    )
    .await
}
