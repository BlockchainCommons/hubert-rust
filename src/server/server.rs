use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::Bytes,
    extract::{ConnectInfo, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use bc_components::ARID;
use bc_envelope::Envelope;
use bc_ur::prelude::*;
use tokio::net::TcpListener;

use crate::{KvStore, SqliteKv};

/// Entry in the server storage.
#[derive(Clone)]
struct StorageEntry {
    envelope_cbor: Vec<u8>, // Store as CBOR bytes (tagged #200)
    expires_at: Option<Instant>,
}

/// Storage backend for the server.
#[derive(Clone)]
enum Storage {
    Memory(Arc<RwLock<HashMap<ARID, StorageEntry>>>),
    Sqlite(SqliteKv),
}

impl Storage {
    fn put_sync(
        &self,
        arid: ARID,
        envelope: Envelope,
        ttl_seconds: u64,
    ) -> Result<(), String> {
        match self {
            Storage::Memory(map) => {
                let mut storage = map.write().unwrap();

                // Check if ARID already exists
                if storage.contains_key(&arid) {
                    return Err("ARID already exists".to_string());
                }

                let expires_at =
                    Instant::now() + Duration::from_secs(ttl_seconds);
                let envelope_cbor = envelope.to_cbor_data();

                storage.insert(
                    arid,
                    StorageEntry {
                        envelope_cbor,
                        expires_at: Some(expires_at),
                    },
                );

                Ok(())
            }
            Storage::Sqlite(store) => {
                // Use tokio's block_in_place to run async code synchronously
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        store
                            .put(&arid, &envelope, Some(ttl_seconds), false)
                            .await
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    })
                })
            }
        }
    }

    fn get_sync(&self, arid: &ARID) -> Option<Envelope> {
        match self {
            Storage::Memory(map) => {
                let mut storage = map.write().unwrap();

                if let Some(entry) = storage.get(arid) {
                    // Check if expired
                    if let Some(expires_at) = entry.expires_at {
                        if Instant::now() >= expires_at {
                            storage.remove(arid);
                            return None;
                        }
                    }

                    // Parse CBOR bytes back to Envelope
                    Envelope::try_from_cbor_data(entry.envelope_cbor.clone())
                        .ok()
                } else {
                    None
                }
            }
            Storage::Sqlite(store) => {
                // Use tokio's block_in_place to run async code synchronously
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        store.get(arid, Some(0), false).await.ok().flatten()
                    })
                })
            }
        }
    }
}

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
    storage: Storage,
    config: ServerConfig,
}

impl ServerState {
    fn new_memory(config: ServerConfig) -> Self {
        Self {
            storage: Storage::Memory(Arc::new(RwLock::new(HashMap::new()))),
            config,
        }
    }

    fn new_sqlite(config: ServerConfig, store: SqliteKv) -> Self {
        Self { storage: Storage::Sqlite(store), config }
    }

    fn put(
        &self,
        arid: ARID,
        envelope: Envelope,
        requested_ttl: Option<Duration>,
        client_ip: Option<SocketAddr>,
    ) -> Result<(), String> {
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

        if self.config.verbose {
            let ip_str = client_ip
                .map(|ip| format!("{}: ", ip))
                .unwrap_or_else(String::new);
            verbose_println(&format!(
                "{}PUT {} (TTL {}s)",
                ip_str,
                arid.ur_string(),
                ttl_seconds
            ));
        }

        self.storage.put_sync(arid, envelope, ttl_seconds)
    }

    fn get(
        &self,
        arid: &ARID,
        client_ip: Option<SocketAddr>,
    ) -> Option<Envelope> {
        use crate::logging::verbose_println;

        if self.config.verbose {
            let ip_str = client_ip
                .map(|ip| format!("{}: ", ip))
                .unwrap_or_else(String::new);
            verbose_println(&format!("{}GET {}", ip_str, arid.ur_string()));
        }

        self.storage.get_sync(arid)
    }
}

/// Hubert HTTP server.
pub struct Server {
    config: ServerConfig,
    state: ServerState,
}

impl Server {
    /// Create a new server with the given configuration.
    /// Uses in-memory storage by default.
    pub fn new(config: ServerConfig) -> Self {
        Self::new_memory(config)
    }

    /// Create a new server with in-memory storage.
    pub fn new_memory(config: ServerConfig) -> Self {
        let state = ServerState::new_memory(config.clone());
        Self { config, state }
    }

    /// Create a new server with SQLite storage.
    pub fn new_sqlite(config: ServerConfig, storage: SqliteKv) -> Self {
        let state = ServerState::new_sqlite(config.clone(), storage);
        Self { config, state }
    }

    /// Run the server.
    pub async fn run(
        self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let app = Router::new()
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
) -> Result<impl IntoResponse, ServerError> {
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
) -> Result<impl IntoResponse, ServerError> {
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
