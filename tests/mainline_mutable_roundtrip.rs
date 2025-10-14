use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use mainline::{Dht, MutableItem, SigningKey, Testnet, async_dht::AsyncDht};
use tokio::time::{Duration, Instant, sleep};

/// Shared test logic: stores a BEP-44 mutable value with a selected key
/// (+salt), then retrieves the most recent item from a separate agent.
async fn mutable_roundtrip_test(
    writer: AsyncDht,
    reader: AsyncDht,
    timeout: Duration,
    delay: Duration,
    poll_interval: Duration,
) -> Result<()> {
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
    // (See crate docs example for lost-update avoidance.)
    let (item, cas) = if let Some(mr) =
        writer.get_mutable_most_recent(&pubkey, salt_opt).await
    {
        let new_seq = mr.seq() + 1;
        (
            MutableItem::new(signing_key, value, new_seq, salt_opt),
            Some(mr.seq()),
        )
    } else {
        (MutableItem::new(signing_key, value, 1, salt_opt), None)
    };

    // Put the mutable item (signed under our selected key).
    writer.put_mutable(item, cas).await?;

    // Allow replication.
    sleep(delay).await;

    // Reader polls the most-recent until it observes our value (soft deadline).
    let start = Instant::now();
    let deadline = start + timeout;
    let mut got = None;
    let mut iterations = 0;

    loop {
        iterations += 1;
        if let Some(mr) =
            reader.get_mutable_most_recent(&pubkey, salt_opt).await
        {
            got = Some(mr);
            break;
        }
        if Instant::now() >= deadline {
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

    let observed =
        got.expect("reader did not observe the mutable item in time");
    assert_eq!(observed.value(), value, "mutable value mismatch");
    // Optional sanity: check we fetched the right channel (pubkey echoed back).
    assert_eq!(observed.key(), &pubkey);

    Ok(())
}

/// Testnet variant: runs against an in-process DHT.
#[tokio::test(flavor = "multi_thread")]
async fn mutable_put_then_get_testnet() -> Result<()> {
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

    mutable_roundtrip_test(
        writer,
        reader,
        Duration::from_secs(5),
        Duration::from_millis(200),
        Duration::from_millis(100),
    )
    .await
}

/// Mainnet variant: hits the real Mainline DHT. Requires outbound UDP.
/// Run with: cargo test -q -- --ignored --nocapture
/// mutable_put_then_get_mainnet
#[tokio::test(flavor = "multi_thread")]
#[ignore = "hits the real Mainline DHT and needs UDP connectivity"]
async fn mutable_put_then_get_mainnet() -> Result<()> {
    // Two separate agents on mainnet using default bootstrap nodes.
    let writer = Dht::client()?.as_async();
    let reader = Dht::client()?.as_async();

    mutable_roundtrip_test(
        writer,
        reader,
        Duration::from_secs(30),
        Duration::from_secs(1),
        Duration::from_millis(250),
    )
    .await
}
