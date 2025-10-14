mod common;

use std::sync::Arc;

use hubert::ipfs::IpfsKv;

/// Test IPFS KV store using the unified test suite.
///
/// These tests validate that IpfsKv correctly implements the KvStore trait
/// with all expected behaviors.
///
/// Requires a running Kubo daemon at 127.0.0.1:5001.
/// Run with: cargo test --test test_ipfs_kv -- --nocapture
async fn setup() -> Option<IpfsKv> {
    // Try to connect to IPFS daemon
    let client = reqwest::Client::new();
    match client
        .post("http://127.0.0.1:5001/api/v0/version")
        .timeout(std::time::Duration::from_secs(1))
        .send()
        .await
    {
        Ok(_) => Some(IpfsKv::new("http://127.0.0.1:5001")),
        Err(_) => None,
    }
}

macro_rules! skip_if_no_ipfs {
    ($store:expr) => {
        match $store {
            Some(s) => s,
            None => {
                eprintln!("⚠️  Skipping test: IPFS daemon not running at 127.0.0.1:5001");
                return;
            }
        }
    };
}

#[tokio::test]
async fn ipfs_basic_roundtrip() {
    let store = skip_if_no_ipfs!(setup().await);
    common::kv_tests::test_basic_roundtrip(&store).await;
}

#[tokio::test]
async fn ipfs_write_once() {
    let store = skip_if_no_ipfs!(setup().await);
    common::kv_tests::test_write_once(&store).await;
}

#[tokio::test]
async fn ipfs_nonexistent_arid() {
    let store = skip_if_no_ipfs!(setup().await);
    common::kv_tests::test_nonexistent_arid(&store).await;
}

#[tokio::test]
async fn ipfs_multiple_arids() {
    let store = skip_if_no_ipfs!(setup().await);
    common::kv_tests::test_multiple_arids(&store).await;
}

#[tokio::test]
async fn ipfs_size_limit() {
    let store = skip_if_no_ipfs!(setup().await);
    common::kv_tests::test_size_limit(&store, 10 * 1024 * 1024).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ipfs_concurrent_operations() {
    if setup().await.is_none() {
        eprintln!(
            "⚠️  Skipping test: IPFS daemon not running at 127.0.0.1:5001"
        );
        return;
    }

    let store1 = Arc::new(IpfsKv::new("http://127.0.0.1:5001"));
    let store2 = Arc::new(IpfsKv::new("http://127.0.0.1:5001"));
    common::kv_tests::test_concurrent_operations(store1, store2).await;
}
