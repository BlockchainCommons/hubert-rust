use anyhow::Result;
use futures_util::TryStreamExt;
use ipfs_api_backend_hyper::{IpfsApi, IpfsClient};
use std::io::Cursor;
use tokio::time::{sleep, Duration};

/// Requires a local Kubo daemon (default RPC at 127.0.0.1:5001).
/// Run with: cargo test -q -- --ignored --nocapture ipfs_immutable_roundtrip
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a running IPFS daemon (kubo) on 127.0.0.1:5001"]
async fn ipfs_immutable_roundtrip() -> Result<()> {
    let client = IpfsClient::default();

    // 1) Add some bytes → CID
    let original: &[u8] = b"hello from ipfs (immutable)";
    let add_res = client.add(Cursor::new(original)).await?;
    let cid = add_res.hash; // e.g., "bafy..." (CIDv1) or "Qm..." (CIDv0)

    // Optional tiny delay (parity with your DHT tests; not strictly needed)
    sleep(Duration::from_millis(100)).await;

    // 2) cat the CID back as a byte stream
    let mut stream = client.cat(&cid);
    let mut roundtrip = Vec::new();
    while let Some(chunk) = stream.try_next().await? {
        roundtrip.extend_from_slice(&chunk);
    }

    assert_eq!(roundtrip.as_slice(), original, "CID roundtrip mismatch");
    Ok(())
}
