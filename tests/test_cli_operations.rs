use anyhow::Result;
use bc_components::ARID;
use bc_envelope::Envelope;
use bc_ur::prelude::*;

mod cli_common;
use cli_common::*;

/// Helper to ensure tags are registered (can be called multiple times safely)
fn ensure_tags_registered() {
    bc_components::register_tags();
}

/// Test basic put and get roundtrip with mainline DHT
///
/// Note: This test requires network access and may be slow (several seconds)
/// as it needs to bootstrap into the mainline DHT testnet.
#[test]
#[ignore] // Ignored by default due to network requirements and slowness
fn test_mainline_put_get_roundtrip() -> Result<()> {
    ensure_tags_registered();

    // Generate test data
    let arid = ARID::new();
    let envelope = Envelope::new("Test message for CLI");

    let arid_ur = arid.ur_string();
    let envelope_ur = envelope.ur_string();

    // Put the envelope
    let put_output = run_cli(&["put", &arid_ur, &envelope_ur])?;
    assert!(
        put_output.contains("Stored envelope") || put_output.contains("✓"),
        "Put should indicate success: {}",
        put_output
    );

    // Get the envelope back
    let get_output = run_cli(&["get", &arid_ur])?;
    assert_eq!(
        get_output, envelope_ur,
        "Retrieved envelope should match original"
    );

    Ok(())
}

/// Test that putting the same ARID twice fails (write-once semantics)
#[test]
#[ignore] // Ignored by default due to network requirements and slowness
fn test_mainline_write_once() -> Result<()> {
    ensure_tags_registered();

    let arid = ARID::new();
    let envelope1 = Envelope::new("First message");
    let envelope2 = Envelope::new("Second message");

    let arid_ur = arid.ur_string();
    let envelope1_ur = envelope1.ur_string();
    let envelope2_ur = envelope2.ur_string();

    // First put should succeed
    run_cli(&["put", &arid_ur, &envelope1_ur])?;

    // Second put to same ARID should fail
    run_cli_expect_error(&["put", &arid_ur, &envelope2_ur])?;

    Ok(())
}

/// Test getting a non-existent ARID
#[test]
#[ignore] // Ignored by default due to network requirements and slowness
fn test_mainline_get_nonexistent() -> Result<()> {
    ensure_tags_registered();

    let arid = ARID::new();
    let arid_ur = arid.ur_string();

    // Getting a non-existent ARID should fail or return nothing
    let result = run_cli(&["get", &arid_ur]);
    assert!(result.is_err(), "Getting non-existent ARID should fail");

    Ok(())
}

/// Test with IPFS storage backend
///
/// Note: This test requires a running IPFS daemon at 127.0.0.1:5001
#[test]
#[ignore] // Ignored by default due to IPFS daemon requirement
fn test_ipfs_put_get_roundtrip() -> Result<()> {
    ensure_tags_registered();

    // Check if IPFS is available first
    if run_cli(&["check", "--storage", "ipfs"]).is_err() {
        println!("Skipping test: IPFS daemon not available");
        return Ok(());
    }

    let arid = ARID::new();
    let envelope = Envelope::new("Test message for IPFS");

    let arid_ur = arid.ur_string();
    let envelope_ur = envelope.ur_string();

    // Put the envelope using IPFS
    run_cli(&["put", "--storage", "ipfs", &arid_ur, &envelope_ur])?;

    // Get the envelope back using IPFS
    let get_output = run_cli(&["get", "--storage", "ipfs", &arid_ur])?;
    assert_eq!(
        get_output, envelope_ur,
        "Retrieved envelope should match original"
    );

    Ok(())
}

/// Test that mixing storage backends fails
/// (can't put to mainline and get from IPFS)
#[test]
#[ignore] // Ignored by default due to network requirements
fn test_storage_backend_isolation() -> Result<()> {
    ensure_tags_registered();

    let arid = ARID::new();
    let envelope = Envelope::new("Backend isolation test");

    let arid_ur = arid.ur_string();
    let envelope_ur = envelope.ur_string();

    // Put to mainline
    run_cli(&["put", "--storage", "mainline", &arid_ur, &envelope_ur])?;

    // Try to get from IPFS - should fail (different storage)
    let result = run_cli(&["get", "--storage", "ipfs", &arid_ur]);
    assert!(
        result.is_err(),
        "Getting from different storage backend should fail"
    );

    Ok(())
}
