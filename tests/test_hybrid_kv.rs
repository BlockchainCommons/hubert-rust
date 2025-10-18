mod common;

use std::sync::Arc;

use bc_components::ARID;
use bc_envelope::Envelope;
use hubert::{KvStore, hybrid::HybridKv};
use mainline::Testnet;

/// Test Hybrid storage layer using the unified test suite.
///
/// These tests validate that HybridKv correctly implements the KvStore
/// trait with automatic size-based routing between DHT and IPFS.
///
/// Requires:
/// - Testnet for DHT (in-process, no external dependencies)
/// - IPFS daemon running at http://127.0.0.1:5001
///
/// Run with: cargo test --test test_hybrid_kv -- --nocapture
///
/// Helper to create a hybrid store
async fn setup() -> HybridKv {
    let _testnet = Testnet::new_async(5).await.unwrap();
    HybridKv::new("http://127.0.0.1:5001").await.unwrap()
}

#[tokio::test]
#[ignore] // Requires IPFS daemon
async fn hybrid_basic_roundtrip() {
    bc_components::register_tags();
    common::kv_tests::test_basic_roundtrip(&setup().await).await;
}

#[tokio::test]
#[ignore] // Requires IPFS daemon
async fn hybrid_write_once() {
    bc_components::register_tags();
    common::kv_tests::test_write_once(&setup().await).await;
}

#[tokio::test]
#[ignore] // Requires IPFS daemon
async fn hybrid_nonexistent_arid() {
    bc_components::register_tags();
    common::kv_tests::test_nonexistent_arid(&setup().await).await;
}

#[tokio::test]
#[ignore] // Requires IPFS daemon
async fn hybrid_multiple_arids() {
    bc_components::register_tags();
    common::kv_tests::test_multiple_arids(&setup().await).await;
}

#[tokio::test]
#[ignore] // Requires IPFS daemon
async fn hybrid_small_envelope_uses_dht_only() {
    bc_components::register_tags();
    let store = setup().await;
    let arid = ARID::new();

    // Small envelope (should fit in DHT)
    let small_envelope = Envelope::new("Small message");

    // Put should succeed and use DHT only
    let result = store.put(&arid, &small_envelope, None, true).await;
    assert!(result.is_ok(), "Put should succeed: {:?}", result.err());

    let receipt = result.unwrap();
    assert!(
        receipt.contains("DHT"),
        "Receipt should indicate DHT storage: {}",
        receipt
    );
    assert!(
        !receipt.contains("IPFS"),
        "Receipt should not mention IPFS for small envelope: {}",
        receipt
    );

    // Verify retrieval
    let retrieved = store
        .get(&arid, Some(10), false)
        .await
        .expect("Get should not error");
    assert!(retrieved.is_some(), "Should retrieve envelope");
    assert_eq!(retrieved.unwrap(), small_envelope);
}

#[tokio::test]
#[ignore] // Requires IPFS daemon
async fn hybrid_large_envelope_uses_ipfs_indirection() {
    bc_components::register_tags();
    let store = setup().await;
    let arid = ARID::new();

    // Large envelope (exceeds DHT limit of 1000 bytes)
    let large_data = "x".repeat(2000);
    let large_envelope = Envelope::new(large_data.clone());

    // Put should succeed and use IPFS with DHT reference
    let result = store.put(&arid, &large_envelope, None, true).await;
    assert!(result.is_ok(), "Put should succeed: {:?}", result.err());

    let receipt = result.unwrap();
    assert!(
        receipt.contains("IPFS"),
        "Receipt should indicate IPFS storage: {}",
        receipt
    );
    assert!(
        receipt.contains("ref:"),
        "Receipt should mention reference ARID: {}",
        receipt
    );

    // Verify retrieval (should transparently handle indirection)
    let retrieved = store
        .get(&arid, Some(30), false)
        .await
        .expect("Get should not error");
    assert!(retrieved.is_some(), "Should retrieve envelope");
    assert_eq!(retrieved.unwrap(), large_envelope);
}

#[tokio::test]
#[ignore] // Requires IPFS daemon
async fn hybrid_boundary_condition() {
    bc_components::register_tags();
    let store = setup().await;

    // Test envelope right at the boundary (1000 bytes)
    // This should fit in DHT
    let boundary_data = "x".repeat(900); // Leave room for envelope overhead
    let boundary_envelope = Envelope::new(boundary_data);

    let arid = ARID::new();
    let result = store.put(&arid, &boundary_envelope, None, true).await;
    assert!(
        result.is_ok(),
        "Boundary envelope should store successfully"
    );

    let retrieved = store
        .get(&arid, Some(10), false)
        .await
        .expect("Get should not error");
    assert!(retrieved.is_some(), "Should retrieve boundary envelope");
    assert_eq!(retrieved.unwrap(), boundary_envelope);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore] // Requires IPFS daemon
async fn hybrid_concurrent_operations() {
    bc_components::register_tags();
    let _testnet = Testnet::new_async(5).await.unwrap();
    let store1 =
        Arc::new(HybridKv::new("http://127.0.0.1:5001").await.unwrap());
    let store2 =
        Arc::new(HybridKv::new("http://127.0.0.1:5001").await.unwrap());
    common::kv_tests::test_concurrent_operations(store1, store2).await;
}

#[tokio::test]
#[ignore] // Requires IPFS daemon
async fn hybrid_exists_check() {
    bc_components::register_tags();
    let store = setup().await;

    let arid1 = ARID::new();
    let arid2 = ARID::new();

    // Check non-existent ARID
    assert!(
        !store.exists(&arid1).await.unwrap(),
        "Non-existent ARID should return false"
    );

    // Store small envelope (DHT only)
    let small = Envelope::new("small");
    store.put(&arid1, &small, None, false).await.unwrap();

    // Check should return true
    assert!(
        store.exists(&arid1).await.unwrap(),
        "Stored ARID should return true"
    );

    // Store large envelope (hybrid)
    let large = Envelope::new("x".repeat(2000));
    store.put(&arid2, &large, None, false).await.unwrap();

    // Check should return true (reference in DHT counts)
    assert!(
        store.exists(&arid2).await.unwrap(),
        "ARID with reference should return true"
    );
}

#[tokio::test]
#[ignore] // Requires IPFS daemon
async fn hybrid_get_timeout() {
    bc_components::register_tags();
    use tokio::time::Instant;

    let store = setup().await;
    let arid = ARID::new(); // Non-existent ARID

    // Measure time to timeout (should be ~2 seconds)
    let start = Instant::now();
    let result = store.get(&arid, Some(2), false).await;
    let elapsed = start.elapsed();

    // Should return None (not found) after timeout
    assert!(
        result.is_ok(),
        "Get should succeed (not error) even on timeout"
    );
    assert!(
        result.unwrap().is_none(),
        "Should return None after timeout"
    );

    // Verify timeout was respected (allow some margin)
    assert!(
        elapsed.as_secs() >= 2 && elapsed.as_secs() <= 4,
        "Timeout should be ~2 seconds, was {} seconds",
        elapsed.as_secs()
    );
}
