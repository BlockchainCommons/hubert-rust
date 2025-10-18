/// Mainline DHT-specific errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Value size {size} exceeds DHT limit of 1000 bytes")]
    ValueTooLarge { size: usize },

    #[error("DHT operation error: {0}")]
    DhtError(String),

    #[error("Put query error: {0}")]
    PutQueryError(#[from] mainline::errors::PutQueryError),

    #[error("Decode ID error: {0}")]
    DecodeIdError(#[from] mainline::errors::DecodeIdError),

    #[error("Put mutable error: {0}")]
    PutMutableError(#[from] mainline::errors::PutMutableError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
