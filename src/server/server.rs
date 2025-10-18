use std::{net::SocketAddr, time::Duration};

use super::{ServerKv, SqliteKv};
use axum::{
    Router,
    body::Bytes,
    extract::{ConnectInfo, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bc_components::ARID;
use bc_envelope::Envelope;
use bc_ur::prelude::*;
use tokio::net::TcpListener;

use crate::Result;

/// Configuration for the Hubert server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub port: u16,
    /// Maximum TTL in seconds allowed.
    /// If a put() specifies a TTL higher than this, it will be clamped.
    /// If put() specifies None, this value will be used.
    /// Hubert is intended for coordination, not long-term storage.
    pub max_ttl: u64,
    /// Enable verbose logging with timestamps
    pub verbose: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 45678,
            max_ttl: 86400, // 24 hours max (and default)
            verbose: false,
        }
    }
}

/// Shared server state.
#[derive(Clone)]
struct ServerState {
    storage: ServerKv,
    config: ServerConfig,
}

impl ServerState {
    fn new(config: ServerConfig, storage: ServerKv) -> Self {
        Self { storage, config }
    }

    fn put(
        &self,
        arid: ARID,
        envelope: Envelope,
        requested_ttl: Option<Duration>,
        client_ip: Option<SocketAddr>,
    ) -> std::result::Result<(), String> {
        use crate::logging::verbose_println;

        // Determine effective TTL:
        // - If requested, use it (clamped to max_ttl)
        // - If None requested, use max_ttl
        // All entries expire (hubert is for coordination, not long-term
        // storage)
        let max_duration = Duration::from_secs(self.config.max_ttl);
        let ttl = match requested_ttl {
            Some(req) => {
                if req > max_duration {
                    max_duration
                } else {
                    req
                }
            }
            None => max_duration,
        };

        let ttl_seconds = ttl.as_secs();

        let result = self.storage.put_sync(arid, envelope, ttl_seconds);

        if self.config.verbose {
            let ip_str =
                client_ip.map(|ip| format!("{}: ", ip)).unwrap_or_default();
            let status = match &result {
                Ok(_) => "OK".to_string(),
                Err(e) => format!("ERROR: {}", e),
            };
            verbose_println(&format!(
                "{}PUT {} (TTL {}s) {}",
                ip_str,
                arid.ur_string(),
                ttl_seconds,
                status
            ));
        }

        result
    }

    fn get(
        &self,
        arid: &ARID,
        client_ip: Option<SocketAddr>,
    ) -> Option<Envelope> {
        use crate::logging::verbose_println;

        let result = self.storage.get_sync(arid);

        if self.config.verbose {
            let ip_str =
                client_ip.map(|ip| format!("{}: ", ip)).unwrap_or_default();
            let status = if result.is_some() { "OK" } else { "NOT_FOUND" };
            verbose_println(&format!(
                "{}GET {} {}",
                ip_str,
                arid.ur_string(),
                status
            ));
        }

        result
    }
}

/// Hubert HTTP server.
pub struct Server {
    config: ServerConfig,
    state: ServerState,
}

impl Server {
    /// Create a new server with the given configuration and storage backend.
    pub fn new(config: ServerConfig, storage: ServerKv) -> Self {
        let state = ServerState::new(config.clone(), storage);
        Self { config, state }
    }

    /// Create a new server with in-memory storage.
    pub fn new_memory(config: ServerConfig) -> Self {
        Self::new(config, ServerKv::memory())
    }

    /// Create a new server with SQLite storage.
    pub fn new_sqlite(config: ServerConfig, storage: SqliteKv) -> Self {
        Self::new(config, ServerKv::sqlite(storage))
    }

    /// Run the server.
    pub async fn run(self) -> Result<()> {
        let app = Router::new()
            .route("/health", get(handle_health))
            .route("/put", post(handle_put))
            .route("/get", post(handle_get))
            .with_state(self.state);

        let addr = format!("127.0.0.1:{}", self.config.port);
        let listener = TcpListener::bind(&addr).await?;
        println!("✓ Hubert server listening on {}", addr);

        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await?;

        Ok(())
    }

    /// Get the port the server is configured to listen on.
    pub fn port(&self) -> u16 {
        self.config.port
    }
}

/// Handle health check requests.
///
/// Returns JSON with server identification and version.
async fn handle_health() -> impl IntoResponse {
    let version = env!("CARGO_PKG_VERSION");
    let response = serde_json::json!({
        "server": "hubert",
        "version": version,
        "status": "ok"
    });
    (StatusCode::OK, serde_json::to_string(&response).unwrap())
}

/// Handle PUT requests.
///
/// Body format:
/// Line 1: ur:arid
/// Line 2: ur:envelope
/// Line 3 (optional): TTL in seconds
async fn handle_put(
    State(state): State<ServerState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    body: Bytes,
) -> std::result::Result<impl IntoResponse, ServerError> {
    // Register tags for UR parsing
    bc_components::register_tags();

    let body_str = String::from_utf8(body.to_vec())
        .map_err(|_| ServerError::BadRequest("Invalid UTF-8".to_string()))?;

    let lines: Vec<&str> = body_str.lines().collect();
    if lines.len() < 2 {
        return Err(ServerError::BadRequest(
            "Expected at least 2 lines: ur:arid and ur:envelope".to_string(),
        ));
    }

    // Parse ARID
    let arid = ARID::from_ur_string(lines[0])
        .map_err(|_| ServerError::BadRequest("Invalid ur:arid".to_string()))?;

    // Parse Envelope
    let envelope = Envelope::from_ur_string(lines[1]).map_err(|_| {
        ServerError::BadRequest("Invalid ur:envelope".to_string())
    })?;

    // Parse optional TTL
    let ttl = if lines.len() > 2 {
        let seconds: u64 = lines[2]
            .parse()
            .map_err(|_| ServerError::BadRequest("Invalid TTL".to_string()))?;
        Some(Duration::from_secs(seconds))
    } else {
        None
    };

    // Store the envelope
    state
        .put(arid, envelope, ttl, Some(addr))
        .map_err(ServerError::Conflict)?;

    Ok((StatusCode::OK, "OK"))
}

/// Handle GET requests.
///
/// Body format:
/// Line 1: ur:arid
async fn handle_get(
    State(state): State<ServerState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    body: Bytes,
) -> std::result::Result<impl IntoResponse, ServerError> {
    // Register tags for UR parsing
    bc_components::register_tags();

    let body_str = String::from_utf8(body.to_vec())
        .map_err(|_| ServerError::BadRequest("Invalid UTF-8".to_string()))?;

    let arid_str = body_str.trim();
    if arid_str.is_empty() {
        return Err(ServerError::BadRequest("Expected ur:arid".to_string()));
    }

    // Parse ARID
    let arid = ARID::from_ur_string(arid_str)
        .map_err(|_| ServerError::BadRequest("Invalid ur:arid".to_string()))?;

    // Retrieve the envelope
    match state.get(&arid, Some(addr)) {
        Some(envelope) => Ok((StatusCode::OK, envelope.ur_string())),
        None => Err(ServerError::NotFound),
    }
}

/// Server error type for HTTP responses.
#[derive(Debug)]
enum ServerError {
    BadRequest(String),
    Conflict(String),
    NotFound,
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        match self {
            ServerError::BadRequest(msg) => {
                (StatusCode::BAD_REQUEST, msg).into_response()
            }
            ServerError::Conflict(msg) => {
                (StatusCode::CONFLICT, msg).into_response()
            }
            ServerError::NotFound => {
                (StatusCode::NOT_FOUND, "Not found").into_response()
            }
        }
    }
}
