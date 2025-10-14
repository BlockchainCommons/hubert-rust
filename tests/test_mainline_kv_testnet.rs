mod common;

use std::sync::Arc;

use hubert::mainline::MainlineDhtKv;
use mainline::Testnet;

/// Test Mainline DHT KV store using the unified test suite.
///
/// These tests validate that MainlineDhtKv correctly implements the KvStore
/// trait with all expected behaviors.
///
/// Uses an in-process testnet (no external dependencies).
/// Run with: cargo test --test test_mainline_kv -- --nocapture
/// Helper to create a testnet-bootstrapped store
async fn setup() -> MainlineDhtKv {
    let _testnet = Testnet::new_async(5).await.unwrap();
    MainlineDhtKv::new().await.unwrap()
}

#[tokio::test]
async fn mainline_basic_roundtrip() {
    common::kv_tests::test_basic_roundtrip(&setup().await).await;
}

#[tokio::test]
async fn mainline_write_once() {
    common::kv_tests::test_write_once(&setup().await).await;
}

#[tokio::test]
async fn mainline_nonexistent_arid() {
    common::kv_tests::test_nonexistent_arid(&setup().await).await;
}

#[tokio::test]
async fn mainline_multiple_arids() {
    common::kv_tests::test_multiple_arids(&setup().await).await;
}

#[tokio::test]
async fn mainline_size_limit() {
    common::kv_tests::test_size_limit(&setup().await, 1000).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn mainline_concurrent_operations() {
    let _testnet = Testnet::new_async(5).await.unwrap();
    let store1 = Arc::new(MainlineDhtKv::new().await.unwrap());
    let store2 = Arc::new(MainlineDhtKv::new().await.unwrap());
    common::kv_tests::test_concurrent_operations(store1, store2).await;
}
