#![allow(dead_code)]

use anyhow::{Result, bail};
use assert_cmd::Command;

/// Run the hubert CLI with the given arguments.
pub fn run_cli_raw(args: &[&str]) -> Result<String> {
    let output = Command::cargo_bin("hubert").unwrap().args(args).assert();

    if output.get_output().status.success() {
        Ok(String::from_utf8(output.get_output().stdout.to_vec()).unwrap())
    } else {
        bail!(
            "Command failed: {:?}",
            String::from_utf8(output.get_output().stderr.to_vec()).unwrap()
        );
    }
}

/// Run the hubert CLI and trim the output.
pub fn run_cli(args: &[&str]) -> Result<String> {
    run_cli_raw(args).map(|s| s.trim().to_string())
}

/// Run the hubert CLI and expect a specific output.
pub fn run_cli_expect(args: &[&str], expected: &str) -> Result<()> {
    let output = run_cli(args)?;
    if output != expected.trim() {
        bail!(
            "\n\n=== Expected ===\n{}\n\n=== Got ===\n{}",
            expected,
            output
        );
    }
    assert_eq!(expected.trim(), output);
    Ok(())
}

/// Run the hubert CLI and expect it to fail.
pub fn run_cli_expect_error(args: &[&str]) -> Result<()> {
    let result = Command::cargo_bin("hubert").unwrap().args(args).assert();

    if result.get_output().status.success() {
        bail!("Expected command to fail, but it succeeded");
    }

    Ok(())
}

/// Check if output contains a specific string.
pub fn run_cli_contains(args: &[&str], expected: &str) -> Result<()> {
    let output = run_cli(args)?;
    if !output.contains(expected) {
        bail!(
            "\n\n=== Expected to contain ===\n{}\n\n=== Got ===\n{}",
            expected,
            output
        );
    }
    Ok(())
}

/// Run the hubert CLI and return output regardless of success/failure.
/// Returns stdout if successful, stderr if failed.
pub fn run_cli_allow_failure(args: &[&str]) -> String {
    let output = Command::cargo_bin("hubert")
        .unwrap()
        .args(args)
        .output()
        .unwrap();

    if output.status.success() {
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    } else {
        String::from_utf8(output.stderr).unwrap().trim().to_string()
    }
}
