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
    // This test will likely fail unless IPFS daemon is running
    let output = run_cli(&["check", "--storage", "ipfs"])?;
    assert!(
        output.contains("IPFS is available")
            || output.contains("not available"),
        "Output should indicate IPFS availability status: {}",
        output
    );
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
