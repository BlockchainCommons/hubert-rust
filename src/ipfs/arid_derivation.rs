use bc_components::ARID;
use bc_crypto::hkdf_hmac_sha256;

/// Salt for deriving IPNS key names from ARIDs.
const HUBERT_IPFS_SALT: &[u8] = b"hubert-ipfs-ipns-v1";

/// Derive a deterministic IPNS key name from an ARID.
///
/// Uses HKDF to derive a key identifier from the ARID, ensuring that:
/// - Same ARID always produces same key name
/// - Key names are cryptographically derived (not guessable)
/// - Collision resistance inherited from ARID
/// - No identifying information in the key name (fully anonymized)
///
/// # Format
///
/// The derived key name is a 64-character hex string (32 bytes of HKDF output).
/// No prefix or identifying markers are included for maximum anonymity.
pub fn derive_key_name(arid: &ARID) -> String {
    let arid_bytes = arid.data();
    let derived = hkdf_hmac_sha256(HUBERT_IPFS_SALT, arid_bytes, 32);
    hex::encode(&derived)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determinism() {
        let arid = ARID::new();
        let key1 = derive_key_name(&arid);
        let key2 = derive_key_name(&arid);
        assert_eq!(key1, key2, "Same ARID must produce same key name");
    }

    #[test]
    fn test_uniqueness() {
        let arid1 = ARID::new();
        let arid2 = ARID::new();
        let key1 = derive_key_name(&arid1);
        let key2 = derive_key_name(&arid2);
        assert_ne!(key1, key2, "Different ARIDs must produce different keys");
    }

    #[test]
    fn test_format() {
        let arid = ARID::new();
        let key = derive_key_name(&arid);
        assert_eq!(key.len(), 64, "Key must be 64 hex characters (32 bytes)");
        // Verify it's valid hex
        assert!(
            key.chars().all(|c| c.is_ascii_hexdigit()),
            "Key must be valid hex"
        );
    }
}
