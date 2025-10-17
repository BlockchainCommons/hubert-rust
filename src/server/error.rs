/// Errors that can occur during server put operations.
#[derive(Debug, thiserror::Error)]
pub enum PutError {
    #[error("{arid} already exists")]
    AlreadyExists { arid: String },

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Parse error: {0}")]
    ParseError(String),
}

impl From<reqwest::Error> for PutError {
    fn from(e: reqwest::Error) -> Self {
        Self::NetworkError(e.to_string())
    }
}

impl From<bc_envelope::Error> for PutError {
    fn from(e: bc_envelope::Error) -> Self {
        Self::ParseError(e.to_string())
    }
}

impl From<dcbor::Error> for PutError {
    fn from(e: dcbor::Error) -> Self {
        Self::ParseError(e.to_string())
    }
}

/// Errors that can occur during server get operations.
#[derive(Debug, thiserror::Error)]
pub enum GetError {
    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Not found")]
    NotFound,
}

impl From<reqwest::Error> for GetError {
    fn from(e: reqwest::Error) -> Self {
        Self::NetworkError(e.to_string())
    }
}

impl From<bc_envelope::Error> for GetError {
    fn from(e: bc_envelope::Error) -> Self {
        Self::ParseError(e.to_string())
    }
}

impl From<dcbor::Error> for GetError {
    fn from(e: dcbor::Error) -> Self {
        Self::ParseError(e.to_string())
    }
}
