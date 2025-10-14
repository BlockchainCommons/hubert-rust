use std::{sync::Arc, thread};

use anyhow::Result;
use bc_components::ARID;
use bc_envelope::Envelope;
use futures_util::future;
use hubert::{KvStore, mainline::MainlineDhtKv};
use mainline::Testnet;
use tokio::sync::mpsc;

/// Helper to get current timestamp in ISO-8601 Zulu format
fn timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Macro for timestamped logging
macro_rules! log {
    ($($arg:tt)*) => {
        println!("[{}] {}", timestamp(), format!($($arg)*))
    };
}

/// Test multi-threaded Mainline DHT KV operations with concurrent tasks.
///
/// This test demonstrates the thread safety and concurrency model of the
/// KvStore trait using Mainline DHT:
///
/// **Architecture:**
/// - Main thread: Generates test data and coordinates
/// - Thread 1 (Put): Sends ARIDs immediately, then spawns 3 concurrent put
///   tasks
/// - Thread 2 (Get): Receives ARIDs, then spawns 3 concurrent polling tasks
///
/// **Flow:**
/// 1. Main thread generates 3 ARIDs and test data
/// 2. Thread 1 immediately sends all ARIDs to Thread 2 as a single message
/// 3. Thread 1 spawns 3 concurrent `spawn_local` tasks to put envelopes
/// 4. Thread 2 receives all ARIDs and spawns 3 concurrent polling tasks
/// 5. Each polling task retries until its envelope appears (or times out)
/// 6. Both threads complete when all tasks finish
/// 7. Main thread verifies all data matches
///
/// **Demonstrates:**
/// - `MainlineDhtKv` is `Send + Sync` (can be shared via `Arc` across threads)
/// - Futures are `!Send` (use `spawn_local` within each thread's runtime)
/// - Multiple concurrent operations per thread work correctly
/// - No data races or synchronization issues
/// - Proper asynchronous coordination between independent threads
///
/// Uses an in-process testnet (no external dependencies).
/// Run with: cargo test -q -- --nocapture mainline_kv_multithreaded
#[tokio::test(flavor = "multi_thread")]
async fn mainline_kv_multithreaded() -> Result<()> {
    // Create testnet for isolated testing
    let _testnet = Testnet::new_async(5).await?;

    // Create stores bootstrapped to the testnet
    let store1 = Arc::new(MainlineDhtKv::new().await?.with_max_size(1000));

    let store2 = Arc::new(MainlineDhtKv::new().await?.with_max_size(1000));

    // Generate test data on main thread
    let test_data = vec![
        ("Alice's data", "Secret message from Alice"),
        ("Bob's data", "Secret message from Bob"),
        ("Carol's data", "Secret message from Carol"),
    ];

    // Generate ARIDs
    let arids: Vec<ARID> = (0..3).map(|_| ARID::new()).collect();

    // Channel to send all ARIDs at once from thread 1 to thread 2
    let (arid_tx, mut arid_rx) = mpsc::channel::<Vec<ARID>>(1);

    // Channel to send results from thread 2 back to main
    let (result_tx, mut result_rx) = mpsc::channel::<(ARID, String)>(10);

    // Thread 1: Put operations (concurrent)
    let arids_clone = arids.clone();
    let test_data_clone = test_data.clone();
    let put_handle = thread::spawn(move || {
        // Create a tokio runtime for this thread
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Send all ARIDs at once to thread 2
            arid_tx.send(arids_clone.clone()).await.unwrap();
            drop(arid_tx);

            log!("Thread 1: Sent all {} ARIDs to thread 2", arids_clone.len());

            // Create local set for spawn_local tasks
            let local_set = tokio::task::LocalSet::new();

            // Spawn concurrent put tasks within the LocalSet
            local_set
                .run_until(async {
                    let mut put_tasks = Vec::new();
                    for (i, arid) in arids_clone.iter().enumerate() {
                        let (subject, body) = test_data_clone[i];
                        let envelope =
                            Envelope::new(subject).add_assertion("body", body);
                        let store_ref = Arc::clone(&store1);
                        let arid_copy = *arid;

                        let task = tokio::task::spawn_local(async move {
                            log!(
                                "Thread 1: Putting ARID {} with subject '{}'",
                                i + 1,
                                subject
                            );

                            match store_ref.put(&arid_copy, &envelope).await {
                                Ok(receipt) => {
                                    log!(
                                        "Thread 1: Put {} successful - {}",
                                        i + 1,
                                        receipt
                                    );
                                    Ok::<(), anyhow::Error>(())
                                }
                                Err(e) => {
                                    log!(
                                        "Thread 1: Put {} failed - {}",
                                        i + 1,
                                        e
                                    );
                                    Err(anyhow::anyhow!("Put failed: {}", e))
                                }
                            }
                        });
                        put_tasks.push(task);
                    }

                    log!(
                        "Thread 1: Waiting for all {} puts to complete...",
                        put_tasks.len()
                    );

                    // Wait for all puts to complete concurrently
                    let results = future::join_all(put_tasks).await;

                    for (i, result) in results.into_iter().enumerate() {
                        result.unwrap()?;
                        log!("Thread 1: Put {} completed", i + 1);
                    }

                    log!("Thread 1: All puts completed successfully");
                    Ok::<(), anyhow::Error>(())
                })
                .await
        })
    });

    // Thread 2: Get operations (concurrent)
    let get_handle = thread::spawn(move || {
        // Create a tokio runtime for this thread
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Receive all ARIDs at once
            log!("Thread 2: Waiting for ARIDs...");
            let arids = arid_rx.recv().await.expect("Failed to receive ARIDs");
            log!("Thread 2: Received {} ARIDs", arids.len());

            // Allow some time for DHT propagation
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            // Create local set for spawn_local tasks
            let local_set = tokio::task::LocalSet::new();

            // Spawn concurrent get tasks within the LocalSet
            local_set.run_until(async {
                let mut get_tasks = Vec::new();
                for (i, arid) in arids.iter().enumerate() {
                    let store_ref = Arc::clone(&store2);
                    let arid_copy = *arid;
                    let result_tx_clone = result_tx.clone();

                    let task = tokio::task::spawn_local(async move {
                        log!("Thread 2: Polling for ARID {}...", i + 1);
                        let max_attempts = 30; // 15 seconds with 500ms polls
                        let mut attempt = 0;

                        loop {
                            attempt += 1;
                            match store_ref.get(&arid_copy).await {
                                Ok(Some(envelope)) => {
                                    // Extract subject
                                    let subject = envelope
                                        .extract_subject::<String>()
                                        .unwrap_or_else(|_| "unknown".to_string());

                                    log!(
                                        "Thread 2: Got ARID {} on attempt {} - subject: '{}'",
                                        i + 1, attempt, subject
                                    );

                                    result_tx_clone.send((arid_copy, subject.clone())).await.unwrap();
                                    return Ok((arid_copy, subject));
                                }
                                Ok(None) => {
                                    if attempt >= max_attempts {
                                        log!(
                                            "Thread 2: Timeout waiting for ARID {} after {} attempts",
                                            i + 1, attempt
                                        );
                                        return Err(anyhow::anyhow!(
                                            "Timeout waiting for ARID {}",
                                            i + 1
                                        ));
                                    }
                                    // Wait before retry
                                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                                }
                                Err(e) => {
                                    log!("Thread 2: Get {} failed - {}", i + 1, e);
                                    return Err(anyhow::anyhow!("Get failed: {}", e));
                                }
                            }
                        }
                    });
                    get_tasks.push(task);
                }

                log!("Thread 2: Waiting for all {} gets to complete...", get_tasks.len());

                // Wait for all gets to complete concurrently
                let results = future::join_all(get_tasks).await;

                let mut received = Vec::new();
                for (i, result) in results.into_iter().enumerate() {
                    match result.unwrap() {
                        Ok(result) => {
                            log!("Thread 2: Get {} completed", i + 1);
                            received.push(result);
                        }
                        Err(e) => {
                            log!("Thread 2: Task {} failed: {}", i + 1, e);
                            return Err(e);
                        }
                    }
                }

                log!("Thread 2: Received all {} envelopes", received.len());
                drop(result_tx); // Signal completion
                Ok(received)
            }).await
        })
    });

    // Wait for thread 1 to complete
    put_handle
        .join()
        .expect("Thread 1 panicked")
        .expect("Thread 1 returned error");

    // Wait for thread 2 to complete
    let received = get_handle
        .join()
        .expect("Thread 2 panicked")
        .expect("Thread 2 returned error");

    // Verify results on main thread
    log!("\nMain thread: Verifying results...");
    assert_eq!(received.len(), 3, "Should have received all 3 envelopes");

    // Collect results from channel
    let mut results = Vec::new();
    while let Some((arid, subject)) = result_rx.recv().await {
        results.push((arid, subject));
    }

    // Verify each ARID matches expected subject
    for (arid, expected_subject) in
        arids.iter().zip(test_data.iter().map(|(s, _)| *s))
    {
        let found = results
            .iter()
            .find(|(recv_arid, _)| recv_arid == arid)
            .expect("ARID not found in results");

        assert_eq!(
            found.1, expected_subject,
            "Subject mismatch for ARID: expected '{}', got '{}'",
            expected_subject, found.1
        );
        log!("✓ ARID verified: {} -> '{}'", arid, found.1);
    }

    log!("\n✓ All data verified successfully!");
    log!("✓ Thread 1 put {} envelopes", arids.len());
    log!("✓ Thread 2 retrieved {} envelopes", received.len());
    log!("✓ Main thread verified all data matches");

    Ok(())
}
