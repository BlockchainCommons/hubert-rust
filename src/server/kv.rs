use bc_components::ARID;
use bc_envelope::Envelope;
use bc_ur::prelude::*;

use super::error::{GetError, PutError};
use crate::KvStore;

/// Server-backed key-value store using HTTP API.
///
/// This implementation communicates with a Hubert server via HTTP POST
/// requests.
///
/// # Example
///
/// ```no_run
/// use bc_components::ARID;
/// use bc_envelope::Envelope;
/// use hubert::{KvStore, server::ServerKv};
///
/// # async fn example() {
/// let store = ServerKv::new("http://127.0.0.1:45678");
/// let arid = ARID::new();
/// let envelope = Envelope::new("Hello, Server!");
///
/// // Put envelope (write-once)
/// store.put(&arid, &envelope, None, false).await.unwrap();
///
/// // Get envelope with verbose logging
/// if let Some(retrieved) = store.get(&arid, None, true).await.unwrap() {
///     assert_eq!(retrieved, envelope);
/// }
/// # }
/// ```
pub struct ServerKv {
    base_url: String,
    client: reqwest::Client,
}

impl ServerKv {
    /// Create a new server KV store client.
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Put an envelope with a TTL (time-to-live).
    ///
    /// Deprecated: Use `KvStore::put(arid, envelope, Some(ttl_seconds))`
    /// instead.
    #[deprecated(
        since = "0.2.0",
        note = "Use KvStore::put() with ttl_seconds parameter instead"
    )]
    pub async fn put_with_ttl(
        &self,
        arid: &ARID,
        envelope: &Envelope,
        ttl_seconds: u64,
    ) -> Result<(), PutError> {
        use crate::KvStore;
        self.put(arid, envelope, Some(ttl_seconds), false)
            .await
            .map(|_| ())
            .map_err(|e| PutError::ServerError(e.to_string()))
    }
}

#[async_trait::async_trait(?Send)]
impl KvStore for ServerKv {
    async fn put(
        &self,
        arid: &ARID,
        envelope: &Envelope,
        ttl_seconds: Option<u64>,
        verbose: bool,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        use crate::logging::{verbose_newline, verbose_println};

        bc_components::register_tags();

        if verbose {
            verbose_println("Starting server put operation");
        }

        // Format body with optional TTL on third line
        let body = if let Some(ttl) = ttl_seconds {
            format!("{}\n{}\n{}", arid.ur_string(), envelope.ur_string(), ttl)
        } else {
            format!("{}\n{}", arid.ur_string(), envelope.ur_string())
        };

        if verbose {
            verbose_println("Sending PUT request to server");
        }

        let response = self
            .client
            .post(format!("{}/put", self.base_url))
            .body(body)
            .send()
            .await
            .map_err(|e| {
                Box::new(PutError::from(e))
                    as Box<dyn std::error::Error + Send + Sync>
            })?;

        let result = match response.status() {
            reqwest::StatusCode::OK => Ok("Stored successfully".to_string()),
            reqwest::StatusCode::CONFLICT => {
                Err(Box::new(PutError::AlreadyExists)
                    as Box<dyn std::error::Error + Send + Sync>)
            }
            _ => {
                let error_msg = response.text().await.unwrap_or_default();
                Err(Box::new(PutError::ServerError(error_msg))
                    as Box<dyn std::error::Error + Send + Sync>)
            }
        };

        if verbose {
            if result.is_ok() {
                verbose_println("Server put operation completed");
            } else {
                verbose_println("Server put operation failed");
            }
            verbose_newline();
        }

        result
    }

    async fn get(
        &self,
        arid: &ARID,
        timeout_seconds: Option<u64>,
        verbose: bool,
    ) -> Result<Option<Envelope>, Box<dyn std::error::Error + Send + Sync>>
    {
        use tokio::time::{Duration, Instant, sleep};

        use crate::logging::{
            verbose_newline, verbose_print_dot, verbose_println,
        };

        bc_components::register_tags();

        if verbose {
            verbose_println("Starting server get operation");
        }

        let timeout = timeout_seconds.unwrap_or(30); // Default 30 seconds
        let deadline = Instant::now() + Duration::from_secs(timeout);
        // Changed to 1000ms for verbose mode polling
        let poll_interval = Duration::from_millis(1000);

        if verbose {
            verbose_println("Polling server for value");
        }

        loop {
            let body = arid.ur_string();

            let response = self
                .client
                .post(format!("{}/get", self.base_url))
                .body(body)
                .send()
                .await
                .map_err(|e| {
                    Box::new(GetError::from(e))
                        as Box<dyn std::error::Error + Send + Sync>
                })?;

            match response.status() {
                reqwest::StatusCode::OK => {
                    if verbose {
                        verbose_newline();
                        verbose_println("Value found on server");
                    }
                    let envelope_str = response.text().await.map_err(|e| {
                        Box::new(GetError::NetworkError(e.to_string()))
                            as Box<dyn std::error::Error + Send + Sync>
                    })?;
                    let envelope = Envelope::from_ur_string(&envelope_str)
                        .map_err(|e| {
                            Box::new(GetError::ParseError(e.to_string()))
                                as Box<dyn std::error::Error + Send + Sync>
                        })?;

                    if verbose {
                        verbose_println("Server get operation completed");
                        verbose_newline();
                    }

                    return Ok(Some(envelope));
                }
                reqwest::StatusCode::NOT_FOUND => {
                    // Not found yet - check if we should keep polling
                    if Instant::now() >= deadline {
                        // Timeout reached
                        if verbose {
                            verbose_newline();
                            verbose_println("Timeout reached, value not found");
                            verbose_newline();
                        }
                        return Ok(None);
                    }

                    // Print polling dot if verbose
                    if verbose {
                        verbose_print_dot();
                    }

                    // Wait before retrying (now 1000ms)
                    sleep(poll_interval).await;
                }
                _ => {
                    let error_msg = response.text().await.unwrap_or_default();
                    return Err(Box::new(GetError::ServerError(error_msg)));
                }
            }
        }
    }

    async fn exists(
        &self,
        arid: &ARID,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        // Use a short timeout for exists check (1 second), no verbose
        Ok(self.get(arid, Some(1), false).await?.is_some())
    }
}
