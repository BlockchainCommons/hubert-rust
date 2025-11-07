/// Hybrid-specific errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Referenced IPFS content not found")]
    ContentNotFound,

    #[error("Not a reference envelope")]
    NotReferenceEnvelope,

    #[error("Invalid ARID in reference envelope")]
    InvalidReferenceArid,

    #[error("No id assertion found in reference envelope")]
    NoIdAssertion,

    #[error("Decrypted envelope is not a valid reference envelope")]
    InvalidDecryptedReference,
}
