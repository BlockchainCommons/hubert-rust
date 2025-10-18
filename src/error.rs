/// Top-level error type for the hubert library.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // Generic storage errors
    #[error("{arid} already exists")]
    AlreadyExists { arid: String },

    #[error("Not found")]
    NotFound,

    #[error("Invalid ARID format")]
    InvalidArid,

    // Dependency errors
    #[error("Envelope error: {0}")]
    Envelope(#[from] bc_envelope::Error),

    #[error("CBOR error: {0}")]
    Cbor(#[from] dcbor::Error),

    // Storage layer-specific errors
    #[error("Mainline DHT error: {0}")]
    Mainline(#[from] crate::mainline::Error),

    #[error("IPFS error: {0}")]
    Ipfs(#[from] crate::ipfs::Error),

    #[error("Server error: {0}")]
    Server(#[from] crate::server::Error),

    #[error("Hybrid error: {0}")]
    Hybrid(#[from] crate::hybrid::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type using the top-level Error.
pub type Result<T> = std::result::Result<T, Error>;
