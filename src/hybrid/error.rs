use thiserror::Error;

/// Errors that can occur when working with reference envelopes
#[derive(Debug, Error)]
pub enum ReferenceError {
    #[error("Not a reference envelope")]
    NotReferenceEnvelope,

    #[error("Invalid ARID in reference envelope")]
    InvalidArid,

    #[error("No id assertion found in reference envelope")]
    NoIdAssertion,

    #[error("Envelope error: {0}")]
    EnvelopeError(String),
}
