/// IPFS-specific errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Envelope size {size} exceeds practical limit")]
    EnvelopeTooLarge { size: usize },

    #[error("IPFS daemon error: {0}")]
    DaemonError(#[from] ipfs_api_backend_hyper::Error),

    #[error("Operation timed out")]
    Timeout,

    #[error("Unexpected IPNS path format: {0}")]
    UnexpectedIpnsPathFormat(String),
}
