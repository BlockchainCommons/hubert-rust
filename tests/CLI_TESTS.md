# CLI Testing for Hubert

This document describes the CLI tests for the `hubert` command-line tool.

## Test Structure

Following the pattern from `bc-envelope-cli`, the CLI tests are organized as follows:

```
hubert/tests/
├── cli_common/           # Shared test utilities
│   └── mod.rs           # CLI command execution helpers
├── test_cli_basic.rs    # Basic CLI functionality tests
├── test_cli_check.rs    # Backend availability tests
└── test_cli_operations.rs  # Put/get integration tests
```

## Test Files

### `cli_common/mod.rs`

Provides helper functions for running CLI commands:

- `run_cli_raw(args)` - Run command and return raw output
- `run_cli(args)` - Run command and return trimmed output
- `run_cli_expect(args, expected)` - Run and assert output matches
- `run_cli_expect_error(args)` - Run and assert command fails
- `run_cli_contains(args, expected)` - Run and assert output contains string

### `test_cli_basic.rs`

Tests fundamental CLI functionality without network access:

- Help text display (`--help`, subcommand help)
- Version display (`--version`)
- Invalid command handling
- Missing argument detection
- Invalid format detection (ARID, envelope)
- Storage backend option validation

**All tests run by default** (no network required)

### `test_cli_check.rs`

Tests the `check` command for backend availability:

- Mainline DHT availability check
- IPFS daemon availability check
- Default storage backend (mainline)
- Short flag (`-s`) support

**All tests run by default** (network checks handle failures gracefully)

### `test_cli_operations.rs`

Tests actual put/get operations across storage backends:

- Mainline DHT put/get roundtrip
- Write-once semantics enforcement
- Non-existent ARID handling
- Hex-encoded ARID support
- IPFS put/get roundtrip
- Storage backend isolation

**All tests are `#[ignore]`d by default** due to:
- Network requirements (DHT bootstrap takes several seconds)
- External dependency requirements (IPFS daemon for some tests)

## Running Tests

### Run all non-ignored tests (fast, no network required):
```bash
cargo test --test test_cli_basic
cargo test --test test_cli_check
```

### Run all CLI tests including ignored ones:
```bash
# Run basic tests (fast)
cargo test --test test_cli_basic

# Run check tests (requires network)
cargo test --test test_cli_check

# Run integration tests (slow, requires network)
cargo test --test test_cli_operations -- --ignored
```

### Run a specific test:
```bash
cargo test --test test_cli_basic test_help
cargo test --test test_cli_operations test_mainline_put_get_roundtrip -- --ignored
```

## Test Categories

### Unit Tests (Fast)
- Command-line parsing
- Help text validation
- Error handling for invalid inputs
- Storage backend selection

**Run time:** < 2 seconds
**Network:** Not required
**External deps:** None

### Integration Tests (Slow)
- Put/get roundtrips with real storage
- Write-once semantics verification
- Cross-backend isolation

**Run time:** 5-30 seconds per test
**Network:** Required (DHT bootstrap)
**External deps:** IPFS daemon (for IPFS tests)

## Adding New Tests

Follow the existing pattern:

```rust
use anyhow::Result;
mod cli_common;
use cli_common::*;

#[test]
fn test_new_feature() -> Result<()> {
    run_cli_contains(&["new-command", "--help"], "expected text")?;
    Ok(())
}

#[test]
#[ignore] // For slow network tests
fn test_network_operation() -> Result<()> {
    // ... network operation
    Ok(())
}
```

## Dependencies

CLI tests use:
- `assert_cmd` - For running the CLI binary in tests
- `anyhow` - For test error handling
- `bc-components` - For ARID and other types
- `bc-envelope` - For Envelope type
- `bc-ur` - For UR encoding/decoding
- `hex` - For hex encoding

All test dependencies are in `[dev-dependencies]`.
