use anyhow::Result;
use bc_components::ARID;
use bc_ur::prelude::*;

mod cli_common;
use cli_common::*;

#[test]
fn test_help() -> Result<()> {
    run_cli_contains(
        &["--help"],
        "Hubert: Secure distributed key-value store",
    )?;
    run_cli_contains(&["--help"], "Commands:")?;
    run_cli_contains(&["--help"], "generate")?;
    run_cli_contains(&["--help"], "put")?;
    run_cli_contains(&["--help"], "get")?;
    run_cli_contains(&["--help"], "check")?;
    Ok(())
}

#[test]
fn test_version() -> Result<()> {
    run_cli_contains(&["--version"], "hubert")?;
    Ok(())
}

#[test]
fn test_put_help() -> Result<()> {
    run_cli_contains(&["put", "--help"], "Store an envelope at an ARID")?;
    run_cli_contains(&["put", "--help"], "<ARID>")?;
    run_cli_contains(&["put", "--help"], "<ENVELOPE>")?;
    Ok(())
}

#[test]
fn test_get_help() -> Result<()> {
    run_cli_contains(&["get", "--help"], "Retrieve an envelope by ARID")?;
    run_cli_contains(&["get", "--help"], "<ARID>")?;
    Ok(())
}

#[test]
fn test_check_help() -> Result<()> {
    run_cli_contains(
        &["check", "--help"],
        "Check if storage backend is available",
    )?;
    run_cli_contains(&["check", "--help"], "--storage")?;
    Ok(())
}

#[test]
fn test_invalid_command() -> Result<()> {
    run_cli_expect_error(&["invalid"])?;
    Ok(())
}

#[test]
fn test_missing_arguments() -> Result<()> {
    run_cli_expect_error(&["put"])?;
    run_cli_expect_error(&["get"])?;
    Ok(())
}

#[test]
fn test_invalid_arid_format() -> Result<()> {
    run_cli_expect_error(&[
        "put",
        "not-a-valid-arid",
        "ur:envelope/tpsoiyfdihjzjzjldmksbaoede",
    ])?;
    run_cli_expect_error(&["get", "not-a-valid-arid"])?;
    Ok(())
}

#[test]
fn test_invalid_envelope_format() -> Result<()> {
    run_cli_expect_error(&[
        "put",
        "ur:arid/hdcxuestvsdemusrdlkngwtosweortdwbasrdrfxhssgfmvlrflthdplatjydmmwahgdwlflguqz",
        "not-a-valid-envelope",
    ])?;
    Ok(())
}

#[test]
fn test_storage_backend_option() -> Result<()> {
    run_cli_contains(&["--storage", "mainline", "--help"], "mainline")?;
    run_cli_contains(&["--storage", "ipfs", "--help"], "ipfs")?;
    Ok(())
}

#[test]
fn test_invalid_storage_backend() -> Result<()> {
    run_cli_expect_error(&["--storage", "invalid", "check"])?;
    Ok(())
}

#[test]
fn test_generate_help() -> Result<()> {
    run_cli_contains(&["generate", "--help"], "Generate a new ARID or example Envelope")?;
    run_cli_contains(&["generate", "--help"], "arid")?;
    Ok(())
}

#[test]
fn test_generate_arid() -> Result<()> {
    // Register tags for UR parsing
    bc_components::register_tags();

    // Generate two ARIDs and verify they're different and valid
    let output1 = run_cli(&["generate", "arid"])?;
    let output2 = run_cli(&["generate", "arid"])?;

    // Should be different
    assert_ne!(output1, output2, "Generated ARIDs should be unique");

    // Should be valid ur:arid format
    assert!(
        output1.starts_with("ur:arid/"),
        "Should start with ur:arid/"
    );
    assert!(
        output2.starts_with("ur:arid/"),
        "Should start with ur:arid/"
    );

    // Should be parseable as ARID
    ARID::from_ur_string(&output1)?;
    ARID::from_ur_string(&output2)?;

    Ok(())
}

#[test]
fn test_hex_arid_not_accepted() -> Result<()> {
    // Hex ARIDs should NOT be accepted (only ur:arid)
    run_cli_expect_error(&[
        "put",
        "dec7e82893c32f7a4fcec633c02c0ec32a4361ca3ee3bc8758ae07742e940550",
        "ur:envelope/tpsoiyfdihjzjzjldmksbaoede",
    ])?;
    Ok(())
}
