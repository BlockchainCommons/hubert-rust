mod common;

use std::sync::Arc;

use hubert::mainline::MainlineDhtKv;
use mainline::Dht;

/// Test Mainline DHT KV store on mainnet using the unified test suite.
///
/// These tests validate that MainlineDhtKv correctly implements the KvStore
/// trait with all expected behaviors on the real Mainline DHT network.
///
/// Requires internet connectivity and UDP access to DHT bootstrap nodes.
/// Run with: cargo test --test test_mainline_kv_mainnet -- --nocapture
///
/// Note: Mainnet tests are slower than testnet tests due to network
/// propagation. Helper to check if we can connect to the mainline DHT
async fn check_mainnet_connectivity() -> bool {
    // Try to create a DHT client and bootstrap
    match Dht::client() {
        Ok(dht) => {
            let async_dht = dht.as_async();
            // Wait up to 5 seconds for bootstrap
            let start = tokio::time::Instant::now();
            let timeout = tokio::time::Duration::from_secs(5);

            while start.elapsed() < timeout {
                if async_dht.bootstrapped().await {
                    return true;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(100))
                    .await;
            }
            false
        }
        Err(_) => false,
    }
}

async fn setup() -> Option<MainlineDhtKv> {
    if !check_mainnet_connectivity().await {
        return None;
    }

    (MainlineDhtKv::new().await).ok()
}

macro_rules! skip_if_no_mainnet {
    ($store:expr) => {
        match $store {
            Some(s) => s,
            None => {
                eprintln!("⚠️  Skipping test: Cannot connect to Mainline DHT (no internet or firewall blocked)");
                return;
            }
        }
    };
}

#[tokio::test]
async fn mainnet_basic_roundtrip() {
    bc_components::register_tags();
    let store = skip_if_no_mainnet!(setup().await);
    common::kv_tests::test_basic_roundtrip(&store).await;
}

#[tokio::test]
async fn mainnet_write_once() {
    bc_components::register_tags();
    let store = skip_if_no_mainnet!(setup().await);
    common::kv_tests::test_write_once(&store).await;
}

#[tokio::test]
async fn mainnet_nonexistent_arid() {
    bc_components::register_tags();
    let store = skip_if_no_mainnet!(setup().await);
    common::kv_tests::test_nonexistent_arid(&store).await;
}

#[tokio::test]
async fn mainnet_multiple_arids() {
    bc_components::register_tags();
    let store = skip_if_no_mainnet!(setup().await);
    common::kv_tests::test_multiple_arids(&store).await;
}

#[tokio::test]
async fn mainnet_size_limit() {
    bc_components::register_tags();
    let store = skip_if_no_mainnet!(setup().await);
    common::kv_tests::test_size_limit(&store, 1000).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn mainnet_concurrent_operations() {
    bc_components::register_tags();
    if setup().await.is_none() {
        eprintln!(
            "⚠️  Skipping test: Cannot connect to Mainline DHT (no internet or firewall blocked)"
        );
        return;
    }

    let store1 = Arc::new(MainlineDhtKv::new().await.unwrap());
    let store2 = Arc::new(MainlineDhtKv::new().await.unwrap());
    common::kv_tests::test_concurrent_operations(store1, store2).await;
}
