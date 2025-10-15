/// Errors that can occur during IPFS put operations.
#[derive(Debug, thiserror::Error)]
pub enum PutError {
    #[error("IPNS name {ipns_name} already published")]
    AlreadyExists { ipns_name: String },

    #[error("Envelope size {size} exceeds practical limit")]
    EnvelopeTooLarge { size: usize },

    #[error("IPFS daemon error: {0}")]
    DaemonError(String),

    #[error("Operation timed out")]
    Timeout,

    #[error("Envelope error: {0}")]
    EnvelopeError(String),

    #[error("CBOR error: {0}")]
    CborError(String),
}

impl From<bc_envelope::Error> for PutError {
    fn from(e: bc_envelope::Error) -> Self {
        Self::EnvelopeError(e.to_string())
    }
}

impl From<dcbor::Error> for PutError {
    fn from(e: dcbor::Error) -> Self { Self::CborError(e.to_string()) }
}

impl From<ipfs_api_backend_hyper::Error> for PutError {
    fn from(e: ipfs_api_backend_hyper::Error) -> Self {
        Self::DaemonError(e.to_string())
    }
}

/// Errors that can occur during IPFS get operations.
#[derive(Debug, thiserror::Error)]
pub enum GetError {
    #[error("IPFS daemon error: {0}")]
    DaemonError(String),

    #[error("IPNS resolution timed out")]
    Timeout,

    #[error("Invalid ARID format")]
    InvalidArid,

    #[error("Envelope error: {0}")]
    EnvelopeError(String),

    #[error("CBOR error: {0}")]
    CborError(String),
}

impl From<bc_envelope::Error> for GetError {
    fn from(e: bc_envelope::Error) -> Self {
        Self::EnvelopeError(e.to_string())
    }
}

impl From<dcbor::Error> for GetError {
    fn from(e: dcbor::Error) -> Self { Self::CborError(e.to_string()) }
}

impl From<ipfs_api_backend_hyper::Error> for GetError {
    fn from(e: ipfs_api_backend_hyper::Error) -> Self {
        Self::DaemonError(e.to_string())
    }
}
