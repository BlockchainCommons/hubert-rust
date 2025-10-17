/// Errors that can occur during Mainline DHT put operations.
#[derive(Debug, thiserror::Error)]
pub enum PutError {
    #[error("{arid} already exists")]
    AlreadyExists { arid: String },

    #[error("Value size {size} exceeds DHT limit of 1000 bytes")]
    ValueTooLarge { size: usize },

    #[error("DHT error: {0}")]
    DhtError(String),

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
    fn from(e: dcbor::Error) -> Self {
        Self::CborError(e.to_string())
    }
}

impl From<mainline::errors::PutQueryError> for PutError {
    fn from(e: mainline::errors::PutQueryError) -> Self {
        Self::DhtError(e.to_string())
    }
}

impl From<mainline::errors::DecodeIdError> for PutError {
    fn from(e: mainline::errors::DecodeIdError) -> Self {
        Self::DhtError(e.to_string())
    }
}

impl From<std::io::Error> for PutError {
    fn from(e: std::io::Error) -> Self {
        Self::DhtError(e.to_string())
    }
}

impl From<mainline::errors::PutMutableError> for PutError {
    fn from(e: mainline::errors::PutMutableError) -> Self {
        Self::DhtError(e.to_string())
    }
}

/// Errors that can occur during Mainline DHT get operations.
#[derive(Debug, thiserror::Error)]
pub enum GetError {
    #[error("DHT error: {0}")]
    DhtError(String),

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
    fn from(e: dcbor::Error) -> Self {
        Self::CborError(e.to_string())
    }
}

impl From<mainline::errors::DecodeIdError> for GetError {
    fn from(e: mainline::errors::DecodeIdError) -> Self {
        Self::DhtError(e.to_string())
    }
}
