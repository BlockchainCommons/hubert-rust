use std::{
    io::Cursor,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use futures_util::TryStreamExt;
use ipfs_api_backend_hyper::{IpfsApi, IpfsClient};
use ipfs_api_prelude::request::KeyType;
use tokio::time::{Duration, Instant, sleep};

// This uses IPNS as the mutable name. It:
// 1. Generates a fresh ed25519 IPNS key,
// 2. Publishes CID₁ to that key, resolves /ipns/<peer_id> → /ipfs/CID₁,
// 3. Publishes CID₂ to the same key, resolves again → /ipfs/CID₂.

/// Helper: collect an IPFS `cat` stream to bytes
async fn cat_all(client: &IpfsClient, cid: &str) -> Result<Vec<u8>> {
    let mut s = client.cat(cid);
    let mut out = Vec::new();
    while let Some(chunk) = s.try_next().await? {
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

/// Requires a local Kubo daemon (default RPC at 127.0.0.1:5001).
/// Run with: cargo test -q -- --ignored --nocapture ipns_mutable_roundtrip
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a running IPFS daemon (kubo) on 127.0.0.1:5001"]
async fn ipns_mutable_roundtrip() -> Result<()> {
    let client = IpfsClient::default();

    // === Prepare two immutable payloads ===
    let a: &[u8] = b"ipns payload A";
    let b: &[u8] = b"ipns payload B (updated)";

    let cid_a = client.add(Cursor::new(a)).await?.hash;
    let cid_b = client.add(Cursor::new(b)).await?.hash;

    // Sanity: both CIDs retrievable
    assert_eq!(cat_all(&client, &cid_a).await?, a);
    assert_eq!(cat_all(&client, &cid_b).await?, b);
    // === Create a dedicated ed25519 IPNS key ===
    let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let key_name = format!("ml-ipns-test-{}", now_ms);
    let key_info = client.key_gen(&key_name, KeyType::Ed25519, 0).await?;
    let peer_id = key_info.id; // the /ipns/<peer_id> name for this key

    // === Publish CID A to IPNS using that key ===
    // name_publish(path, resolve, lifetime, ttl, key)
    // - resolve=false: don’t try to resolve input path
    // - lifetime=None: default Kubo lifetime (24h)
    // - ttl=None: no explicit TTL hint
    // - key=Some(&key_name): use our generated key
    let _pub_a = client
        .name_publish(
            &format!("/ipfs/{}", cid_a),
            false,
            None,
            None,
            Some(&key_name),
        )
        .await?;

    // Give publisher a moment to settle locally.
    sleep(Duration::from_millis(200)).await;

    // === Resolve /ipns/<peer_id> to CID A (poll with soft timeout) ===
    let expect_path_a = format!("/ipfs/{}", cid_a);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let res = client
            // name_resolve(name, recursive, nocache, dht_record_count,
            // dht_timeout)
            // - name: Some(peer_id) to resolve /ipns/<peer_id>
            // - recursive=false: single-hop resolve
            // - nocache=false: allow local cache (fast on same node)
            .name_resolve(Some(&peer_id), false, false)
            .await?;

        if res.path == expect_path_a {
            break;
        }
        if Instant::now() >= deadline {
            panic!(
                "IPNS did not resolve to CID A within timeout; got {}",
                res.path
            );
        }
        sleep(Duration::from_millis(250)).await;
    }

    // === Update: publish CID B to the same key ===
    let _pub_b = client
        .name_publish(
            &format!("/ipfs/{}", cid_b),
            false,
            None,
            None,
            Some(&key_name),
        )
        .await?;
    sleep(Duration::from_millis(200)).await;

    // === Re-resolve and expect CID B ===
    let expect_path_b = format!("/ipfs/{}", cid_b);
    let deadline2 = Instant::now() + Duration::from_secs(10);
    loop {
        let res = client.name_resolve(Some(&peer_id), false, true).await?;

        if res.path == expect_path_b {
            break;
        }
        if Instant::now() >= deadline2 {
            panic!(
                "IPNS did not advance to CID B within timeout; got {}",
                res.path
            );
        }
        sleep(Duration::from_millis(250)).await;
    }

    // Final sanity: fetch the currently-resolved object by CID B
    assert_eq!(cat_all(&client, &cid_b).await?, b);
    Ok(())
}
