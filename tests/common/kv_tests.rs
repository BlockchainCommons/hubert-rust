/// Unified test suite for KvStore implementations.
use std::{sync::Arc, thread};

use bc_components::ARID;
use bc_envelope::Envelope;
use futures_util::future;
use hubert::KvStore;
use tokio::sync::mpsc;

/// Poll for an envelope with retries.
async fn poll_for_envelope(
    store: &impl KvStore,
    arid: ARID,
    index: usize,
    result_tx: mpsc::Sender<(ARID, String)>,
) {
    const MAX_ATTEMPTS: u32 = 30;
    const RETRY_DELAY_MS: u64 = 500;

    for attempt in 1..=MAX_ATTEMPTS {
        match store.get(&arid, Some(30), false).await {
            Ok(Some(envelope)) => {
                let subject: String = envelope.extract_subject().unwrap();
                result_tx.send((arid, subject)).await.unwrap();
                return;
            }
            Ok(None) if attempt < MAX_ATTEMPTS => {
                tokio::time::sleep(tokio::time::Duration::from_millis(
                    RETRY_DELAY_MS,
                ))
                .await;
            }
            _ => {
                panic!("Get failed for ARID {}", index + 1);
            }
        }
    }
}

pub async fn test_basic_roundtrip(store: &impl KvStore) {
    let arid = ARID::new();
    let envelope = Envelope::new("Test").add_assertion("key", "value");

    assert!(!store.exists(&arid).await.unwrap());
    store.put(&arid, &envelope, None, false).await.unwrap();
    assert!(store.exists(&arid).await.unwrap());

    let retrieved = store.get(&arid, Some(30), false).await.unwrap().unwrap();
    assert_eq!(retrieved, envelope);
    println!("✓ Basic roundtrip test passed");
}

pub async fn test_write_once(store: &impl KvStore) {
    let arid = ARID::new();
    store
        .put(&arid, &Envelope::new("First"), None, false)
        .await
        .unwrap();
    assert!(
        store
            .put(&arid, &Envelope::new("Second"), None, false)
            .await
            .is_err()
    );
    println!("✓ Write-once test passed");
}

pub async fn test_nonexistent_arid(store: &impl KvStore) {
    let arid = ARID::new();
    assert!(!store.exists(&arid).await.unwrap());
    assert!(store.get(&arid, Some(30), false).await.unwrap().is_none());
    println!("✓ Non-existent ARID test passed");
}

pub async fn test_multiple_arids(store: &impl KvStore) {
    let arids: Vec<_> = (0..5).map(|_| ARID::new()).collect();
    for (i, arid) in arids.iter().enumerate() {
        store
            .put(
                arid,
                &Envelope::new(format!("Msg {}", i).as_str()),
                None,
                false,
            )
            .await
            .unwrap();
    }
    println!("✓ Multiple ARIDs test passed");
}

pub async fn test_size_limit(store: &impl KvStore, max_size: usize) {
    let arid = ARID::new();
    let large = Envelope::new("x".repeat(max_size + 1000).as_str());
    assert!(store.put(&arid, &large, None, false).await.is_err());
    println!("✓ Size limit test passed");
}

/// Test multi-threaded concurrent operations.
///
/// This test demonstrates the thread safety and concurrency model of KvStore:
///
/// **Architecture:**
/// - Thread 1: Spawns concurrent put tasks
/// - Thread 2: Spawns concurrent get/polling tasks
/// - Main: Verifies all data matches
///
/// **Demonstrates:**
/// - `KvStore` is `Send + Sync` (shareable via `Arc`)
/// - Futures are `!Send` (use `spawn_local` per thread)
/// - Multiple concurrent operations work correctly
/// - No data races or synchronization issues
pub async fn test_concurrent_operations<S>(store1: Arc<S>, store2: Arc<S>)
where
    S: KvStore + 'static,
{
    let test_data = vec![
        ("Alice's data", "Secret from Alice"),
        ("Bob's data", "Secret from Bob"),
        ("Carol's data", "Secret from Carol"),
    ];

    let arids: Vec<ARID> = (0..3).map(|_| ARID::new()).collect();
    let (arid_tx, mut arid_rx) = mpsc::channel::<Vec<ARID>>(1);
    let (result_tx, mut result_rx) = mpsc::channel::<(ARID, String)>(10);

    // Thread 1: Put operations
    let arids_clone = arids.clone();
    let test_data_clone = test_data.clone();
    let put_handle = thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            arid_tx.send(arids_clone.clone()).await.unwrap();
            drop(arid_tx);

            let local_set = tokio::task::LocalSet::new();
            local_set
                .run_until(async {
                    let tasks: Vec<_> = arids_clone
                        .iter()
                        .enumerate()
                        .map(|(i, arid)| {
                            let (subject, body) = test_data_clone[i];
                            let envelope = Envelope::new(subject)
                                .add_assertion("body", body);
                            let store_ref = Arc::clone(&store1);
                            let arid_copy = *arid;

                            tokio::task::spawn_local(async move {
                                store_ref
                                    .put(&arid_copy, &envelope, None, false)
                                    .await
                                    .unwrap();
                            })
                        })
                        .collect();

                    future::join_all(tasks).await;
                })
                .await
        })
    });

    // Thread 2: Get operations with polling
    let get_handle = thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let arids = arid_rx.recv().await.expect("Failed to receive ARIDs");

            // Small delay for propagation
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            let local_set = tokio::task::LocalSet::new();
            local_set
                .run_until(async {
                    let tasks: Vec<_> = arids
                        .iter()
                        .enumerate()
                        .map(|(i, arid)| {
                            let store_ref = Arc::clone(&store2);
                            let arid_copy = *arid;
                            let result_tx_clone = result_tx.clone();

                            tokio::task::spawn_local(async move {
                                poll_for_envelope(
                                    &*store_ref,
                                    arid_copy,
                                    i,
                                    result_tx_clone,
                                )
                                .await
                            })
                        })
                        .collect();

                    future::join_all(tasks).await;
                    drop(result_tx);
                })
                .await
        })
    });

    put_handle.join().expect("Thread 1 panicked");
    get_handle.join().expect("Thread 2 panicked");

    // Verify results
    let mut results = Vec::new();
    while let Some((arid, subject)) = result_rx.recv().await {
        results.push((arid, subject));
    }

    assert_eq!(results.len(), 3, "Should receive all 3 envelopes");

    for (arid, expected_subject) in
        arids.iter().zip(test_data.iter().map(|(s, _)| *s))
    {
        let found = results
            .iter()
            .find(|(recv_arid, _)| recv_arid == arid)
            .expect("ARID not found");
        assert_eq!(found.1, expected_subject);
    }

    println!("✓ Concurrent operations test passed");
}
