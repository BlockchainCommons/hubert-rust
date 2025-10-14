use std::{sync::Arc, thread};

use anyhow::Result;
use bc_components::ARID;
use bc_envelope::Envelope;
use hubert::{KvStore, ipfs::IpfsKv};
use tokio::sync::mpsc;

/// Test multi-threaded IPFS KV operations with separate put/get threads.
///
/// This test demonstrates:
/// - Thread safety of IpfsKv (Send + Sync)
/// - Concurrent put operations on thread 1
/// - Concurrent get operations on thread 2
/// - Proper coordination via channels
///
/// Requires a local Kubo daemon (default RPC at 127.0.0.1:5001).
/// Run with: cargo test -q -- --ignored --nocapture ipfs_kv_multithreaded
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a running IPFS daemon (kubo) on 127.0.0.1:5001"]
async fn ipfs_kv_multithreaded() -> Result<()> {
    // Create shared store
    let store = Arc::new(IpfsKv::new("http://127.0.0.1:5001"));

    // Generate test data on main thread
    let test_data = vec![
        ("Alice's data", "Secret message from Alice"),
        ("Bob's data", "Secret message from Bob"),
        ("Carol's data", "Secret message from Carol"),
    ];

    // Generate ARIDs
    let arids: Vec<ARID> = (0..3).map(|_| ARID::new()).collect();

    // Channel to send ARIDs from thread 1 to thread 2
    let (arid_tx, mut arid_rx) = mpsc::channel::<ARID>(10);

    // Channel to send results from thread 2 back to main
    let (result_tx, mut result_rx) = mpsc::channel::<(ARID, String)>(10);

    // Thread 1: Put operations
    let store1 = Arc::clone(&store);
    let arids_clone = arids.clone();
    let test_data_clone = test_data.clone();
    let put_handle = thread::spawn(move || {
        // Create a tokio runtime for this thread
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Immediately send ARIDs to thread 2
            for arid in &arids_clone {
                arid_tx.send(*arid).await.unwrap();
            }
            drop(arid_tx); // Close channel to signal no more ARIDs

            println!("Thread 1: Sent all ARIDs to thread 2");

            // Now perform puts
            for (i, arid) in arids_clone.iter().enumerate() {
                let (subject, body) = test_data_clone[i];
                let envelope =
                    Envelope::new(subject).add_assertion("body", body);

                println!(
                    "Thread 1: Putting ARID {} with subject '{}'",
                    i + 1,
                    subject
                );

                match store1.put(arid, &envelope).await {
                    Ok(receipt) => {
                        println!("Thread 1: Put successful - {}", receipt);
                    }
                    Err(e) => {
                        eprintln!("Thread 1: Put failed - {}", e);
                        return Err::<(), anyhow::Error>(anyhow::anyhow!(
                            "Put failed: {}",
                            e
                        ));
                    }
                }
            }

            println!("Thread 1: Completed all puts");
            Ok(())
        })
    });

    // Thread 2: Get operations
    let store2 = Arc::clone(&store);
    let get_handle = thread::spawn(move || {
        // Create a tokio runtime for this thread
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut received = Vec::new();
            let mut arid_count = 0;

            // Receive ARIDs as they arrive
            println!("Thread 2: Waiting for ARIDs...");
            while let Some(arid) = arid_rx.recv().await {
                arid_count += 1;
                println!("Thread 2: Received ARID {}", arid_count);

                // Poll for the envelope with retries
                println!("Thread 2: Polling for ARID {}...", arid_count);
                let max_attempts = 60; // 30 seconds with 500ms polls
                let mut attempt = 0;

                loop {
                    attempt += 1;
                    match store2.get(&arid).await {
                        Ok(Some(envelope)) => {
                            // Extract subject
                            let subject = envelope
                                .extract_subject::<String>()
                                .unwrap_or_else(|_| "unknown".to_string());

                            println!(
                                "Thread 2: Got ARID {} on attempt {} - subject: '{}'",
                                arid_count, attempt, subject
                            );

                            received.push((arid, subject.clone()));
                            result_tx.send((arid, subject)).await.unwrap();
                            break;
                        }
                        Ok(None) => {
                            if attempt >= max_attempts {
                                eprintln!(
                                    "Thread 2: Timeout waiting for ARID {} after {} attempts",
                                    arid_count, attempt
                                );
                                return Err::<Vec<(ARID, String)>, anyhow::Error>(
                                    anyhow::anyhow!("Timeout waiting for ARID {}", arid_count),
                                );
                            }
                            // Wait before retry
                            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                        }
                        Err(e) => {
                            eprintln!("Thread 2: Get failed - {}", e);
                            return Err(anyhow::anyhow!("Get failed: {}", e));
                        }
                    }
                }
            }

            println!("Thread 2: Received all {} envelopes", received.len());
            drop(result_tx); // Signal completion
            Ok(received)
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
    println!("\nMain thread: Verifying results...");
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
        println!("✓ ARID verified: {} -> '{}'", arid, found.1);
    }

    println!("\n✓ All data verified successfully!");
    println!("✓ Thread 1 put {} envelopes", arids.len());
    println!("✓ Thread 2 retrieved {} envelopes", received.len());
    println!("✓ Main thread verified all data matches");

    Ok(())
}
