/// Server-specific errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Server error: {0}")]
    General(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Reqwest error: {0}")]
    ReqwestError(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("System time error: {0}")]
    SystemTimeError(#[from] std::time::SystemTimeError),
}
