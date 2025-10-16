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
/// store.put(&arid, &envelope, None).await.unwrap();
///
/// // Get envelope
/// if let Some(retrieved) = store.get(&arid, None).await.unwrap() {
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
        self.put(arid, envelope, Some(ttl_seconds))
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
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        bc_components::register_tags();

        // Format body with optional TTL on third line
        let body = if let Some(ttl) = ttl_seconds {
            format!("{}\n{}\n{}", arid.ur_string(), envelope.ur_string(), ttl)
        } else {
            format!("{}\n{}", arid.ur_string(), envelope.ur_string())
        };

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

        match response.status() {
            reqwest::StatusCode::OK => Ok("Stored successfully".to_string()),
            reqwest::StatusCode::CONFLICT => {
                Err(Box::new(PutError::AlreadyExists))
            }
            _ => {
                let error_msg = response.text().await.unwrap_or_default();
                Err(Box::new(PutError::ServerError(error_msg)))
            }
        }
    }

    async fn get(
        &self,
        arid: &ARID,
        timeout_seconds: Option<u64>,
    ) -> Result<Option<Envelope>, Box<dyn std::error::Error + Send + Sync>>
    {
        use tokio::time::{Duration, Instant, sleep};

        bc_components::register_tags();

        let timeout = timeout_seconds.unwrap_or(30); // Default 30 seconds
        let deadline = Instant::now() + Duration::from_secs(timeout);
        let poll_interval = Duration::from_millis(500);

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
                    let envelope_str = response.text().await.map_err(|e| {
                        Box::new(GetError::NetworkError(e.to_string()))
                            as Box<dyn std::error::Error + Send + Sync>
                    })?;
                    let envelope = Envelope::from_ur_string(&envelope_str)
                        .map_err(|e| {
                            Box::new(GetError::ParseError(e.to_string()))
                                as Box<dyn std::error::Error + Send + Sync>
                        })?;
                    return Ok(Some(envelope));
                }
                reqwest::StatusCode::NOT_FOUND => {
                    // Not found yet - check if we should keep polling
                    if Instant::now() >= deadline {
                        // Timeout reached
                        return Ok(None);
                    }
                    // Wait before retrying
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
        // Use a short timeout for exists check (1 second)
        Ok(self.get(arid, Some(1)).await?.is_some())
    }
}
