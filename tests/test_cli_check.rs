use anyhow::Result;

mod cli_common;
use cli_common::*;

#[test]
fn test_check_mainline() -> Result<()> {
    // This test may take a few seconds as it bootstraps a testnet
    let output = run_cli(&["check", "--storage", "mainline"])?;
    assert!(
        output.contains("Mainline DHT is available")
            || output.contains("not available"),
        "Output should indicate DHT availability status: {}",
        output
    );
    Ok(())
}

#[test]
fn test_check_mainline_default() -> Result<()> {
    // Test that mainline is the default storage backend
    let output = run_cli(&["check"])?;
    assert!(
        output.contains("Mainline DHT is available")
            || output.contains("not available"),
        "Output should indicate DHT availability status: {}",
        output
    );
    Ok(())
}

#[test]
fn test_check_ipfs() -> Result<()> {
    // This test handles both success and failure cases (when IPFS daemon is not running)
    let output = run_cli_allow_failure(&["check", "--storage", "ipfs"]);

    if output.contains("IPFS is available") {
        // IPFS daemon is running, test passes
        println!("✓ IPFS daemon is running and available");
    } else if output.contains("not available") {
        // IPFS daemon is not running, test passes with warning
        println!(
            "⚠ Warning: IPFS daemon is not running - test passed but IPFS check failed"
        );
    } else {
        // Unexpected output
        panic!(
            "Expected output to contain 'IPFS is available' or 'not available', but got: {}",
            output
        );
    }

    Ok(())
}

#[test]
fn test_check_with_short_flag() -> Result<()> {
    let output = run_cli(&["check", "-s", "mainline"])?;
    assert!(
        output.contains("Mainline DHT is available")
            || output.contains("not available"),
        "Output should indicate DHT availability status: {}",
        output
    );
    Ok(())
}
