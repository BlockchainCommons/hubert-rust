use anyhow::Result;
use bc_components::ARID;
use bc_envelope::Envelope;
use hubert::{
    KvStore,
    server::{Server, ServerConfig, ServerKv},
};
use tokio::time::{Duration, sleep};

/// Test basic put/get roundtrip with in-process server
#[tokio::test]
async fn test_server_put_get_roundtrip() -> Result<()> {
    // Register tags for UR parsing
    bc_components::register_tags();

    // Start server in background
    let config = ServerConfig::default();
    let server = Server::new(config.clone());

    tokio::spawn(async move { server.run().await });

    // Wait for server to start
    sleep(Duration::from_millis(100)).await;

    // Create client
    let client = ServerKv::new(&format!("http://127.0.0.1:{}", config.port));

    // Generate test data
    let arid = ARID::new();
    let envelope = Envelope::new("Test message for server");

    // Put the envelope
    let receipt = client
        .put(&arid, &envelope, None, false) // No TTL
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(!receipt.is_empty(), "Receipt should not be empty");

    // Get the envelope back
    let retrieved = client
        .get(&arid, Some(30), false)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(retrieved.is_some(), "Envelope should be retrieved");
    assert_eq!(
        retrieved.unwrap(),
        envelope,
        "Retrieved envelope should match original"
    );

    Ok(())
}

/// Test write-once semantics (putting same ARID twice should fail)
#[tokio::test]
async fn test_server_write_once() -> Result<()> {
    bc_components::register_tags();

    let config = ServerConfig { port: 45680, ..Default::default() };
    let server = Server::new(config.clone());

    tokio::spawn(async move { server.run().await });

    sleep(Duration::from_millis(100)).await;

    let client = ServerKv::new(&format!("http://127.0.0.1:{}", config.port));

    let arid = ARID::new();
    let envelope1 = Envelope::new("First message");
    let envelope2 = Envelope::new("Second message");

    // First put should succeed
    client
        .put(&arid, &envelope1, None, false) // No TTL
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Second put to same ARID should fail
    let result = client.put(&arid, &envelope2, None, false).await;
    assert!(result.is_err(), "Second put should fail");

    Ok(())
}

/// Test getting non-existent ARID
#[tokio::test]
async fn test_server_get_nonexistent() -> Result<()> {
    bc_components::register_tags();

    let config = ServerConfig { port: 45681, ..Default::default() };
    let server = Server::new(config.clone());

    tokio::spawn(async move { server.run().await });

    sleep(Duration::from_millis(100)).await;

    let client = ServerKv::new(&format!("http://127.0.0.1:{}", config.port));

    let arid = ARID::new();
    let retrieved = client
        .get(&arid, Some(30), false)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(retrieved.is_none(), "Non-existent ARID should return None");

    Ok(())
}

/// Test TTL expiration
#[tokio::test]
async fn test_server_ttl() -> Result<()> {
    bc_components::register_tags();

    let config = ServerConfig { port: 45682, ..Default::default() };
    let server = Server::new(config.clone());

    tokio::spawn(async move { server.run().await });

    sleep(Duration::from_millis(100)).await;

    let client = ServerKv::new(&format!("http://127.0.0.1:{}", config.port));

    let arid = ARID::new();
    let envelope = Envelope::new("Message with TTL");

    // Put with 1 second TTL
    client
        .put(&arid, &envelope, Some(1), false) // 1 second TTL
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Should be available immediately
    let retrieved = client
        .get(&arid, Some(30), false)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(retrieved.is_some(), "Envelope should be available");

    // Wait for expiration
    sleep(Duration::from_secs(2)).await;

    // Should be expired
    let retrieved = client
        .get(&arid, Some(30), false)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(retrieved.is_none(), "Envelope should be expired");

    Ok(())
}

/// Test that None TTL uses max_ttl from config
#[tokio::test]
async fn test_server_default_ttl() -> Result<()> {
    bc_components::register_tags();

    // Configure server with short max_ttl for testing
    let config = ServerConfig {
        port: 45683,
        max_ttl: 2, // 2 seconds
        verbose: false,
    };
    let server = Server::new(config.clone());

    tokio::spawn(async move { server.run().await });

    sleep(Duration::from_millis(100)).await;

    let client = ServerKv::new(&format!("http://127.0.0.1:{}", config.port));

    let arid = ARID::new();
    let envelope = Envelope::new("Message with default TTL");

    // Put with None (should use max_ttl = 2 seconds)
    client
        .put(&arid, &envelope, None, false)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Should be available immediately
    let retrieved = client
        .get(&arid, Some(30), false)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(retrieved.is_some(), "Envelope should be available");

    // Wait for expiration (max_ttl = 2 seconds)
    sleep(Duration::from_secs(3)).await;

    // Should be expired
    let retrieved = client
        .get(&arid, Some(30), false)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(
        retrieved.is_none(),
        "Envelope should be expired after max_ttl"
    );

    Ok(())
}

/// Test that TTL is clamped to max_ttl
#[tokio::test]
async fn test_server_ttl_clamping() -> Result<()> {
    bc_components::register_tags();

    // Configure server with short max_ttl for testing
    let config = ServerConfig {
        port: 45684,
        max_ttl: 2, // 2 seconds max
        verbose: false,
    };
    let server = Server::new(config.clone());

    tokio::spawn(async move { server.run().await });

    sleep(Duration::from_millis(100)).await;

    let client = ServerKv::new(&format!("http://127.0.0.1:{}", config.port));

    let arid = ARID::new();
    let envelope = Envelope::new("Message with clamped TTL");

    // Put with 10 seconds (should be clamped to 2 seconds)
    client
        .put(&arid, &envelope, Some(10), false)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Should be available immediately
    let retrieved = client
        .get(&arid, Some(30), false)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(retrieved.is_some(), "Envelope should be available");

    // Wait for clamped TTL (2 seconds, not 10)
    sleep(Duration::from_secs(3)).await;

    // Should be expired after 2 seconds (not 10)
    let retrieved = client
        .get(&arid, Some(30), false)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(
        retrieved.is_none(),
        "Envelope should be expired after max_ttl (clamped)"
    );

    Ok(())
}

/// Test get timeout polling behavior
#[tokio::test]
async fn test_server_get_timeout() -> Result<()> {
    use tokio::time::Instant;

    bc_components::register_tags();

    let config = ServerConfig { port: 45685, max_ttl: 86400, verbose: false };
    let server = Server::new(config.clone());

    tokio::spawn(async move { server.run().await });

    sleep(Duration::from_millis(100)).await;

    let client = ServerKv::new(&format!("http://127.0.0.1:{}", config.port));

    let arid = ARID::new(); // ARID that doesn't exist

    // Measure time to timeout (should be ~2 seconds)
    let start = Instant::now();
    let result = client.get(&arid, Some(2), false).await;
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
        elapsed.as_secs() >= 2 && elapsed.as_secs() <= 3,
        "Timeout should be ~2 seconds, was {} seconds",
        elapsed.as_secs()
    );

    Ok(())
}
