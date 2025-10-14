use bc_components::ARID;
use bc_crypto::hkdf_hmac_sha256;

/// Derive a deterministic key identifier from an ARID using a specific salt.
///
/// Uses HKDF to derive a key identifier from the ARID, ensuring that:
/// - Same ARID always produces same key for a given salt
/// - Keys are cryptographically derived (not guessable)
/// - Collision resistance inherited from ARID
/// - No identifying information in the key (fully anonymized)
///
/// # Parameters
///
/// - `salt`: Domain-specific salt to ensure different backends derive different
///   keys
/// - `arid`: The ARID to derive from
/// - `output_len`: Length of output in bytes (typically 20 or 32)
///
/// # Returns
///
/// Hex-encoded derived key
pub fn derive_key(salt: &[u8], arid: &ARID, output_len: usize) -> String {
    let arid_bytes = arid.data();
    let derived = hkdf_hmac_sha256(salt, arid_bytes, output_len);
    hex::encode(&derived)
}

/// Derive an IPNS key name from an ARID.
///
/// Returns a 64-character hex string (32 bytes).
pub fn derive_ipfs_key_name(arid: &ARID) -> String {
    const SALT: &[u8] = b"hubert-ipfs-ipns-v1";
    derive_key(SALT, arid, 32)
}

/// Derive a Mainline DHT key from an ARID.
///
/// Returns a 40-character hex string (20 bytes, SHA-1 compatible).
pub fn derive_mainline_key(arid: &ARID) -> String {
    const SALT: &[u8] = b"hubert-mainline-dht-v1";
    derive_key(SALT, arid, 20)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determinism() {
        let arid = ARID::new();
        let key1 = derive_ipfs_key_name(&arid);
        let key2 = derive_ipfs_key_name(&arid);
        assert_eq!(key1, key2, "Same ARID must produce same key");

        let key3 = derive_mainline_key(&arid);
        let key4 = derive_mainline_key(&arid);
        assert_eq!(key3, key4, "Same ARID must produce same key");
    }

    #[test]
    fn test_uniqueness() {
        let arid1 = ARID::new();
        let arid2 = ARID::new();
        let ipfs1 = derive_ipfs_key_name(&arid1);
        let ipfs2 = derive_ipfs_key_name(&arid2);
        assert_ne!(ipfs1, ipfs2, "Different ARIDs must produce different keys");

        let ml1 = derive_mainline_key(&arid1);
        let ml2 = derive_mainline_key(&arid2);
        assert_ne!(ml1, ml2, "Different ARIDs must produce different keys");
    }

    #[test]
    fn test_format_ipfs() {
        let arid = ARID::new();
        let key = derive_ipfs_key_name(&arid);
        assert_eq!(key.len(), 64, "IPFS key must be 64 hex characters");
        assert!(
            key.chars().all(|c| c.is_ascii_hexdigit()),
            "Key must be valid hex"
        );
    }

    #[test]
    fn test_format_mainline() {
        let arid = ARID::new();
        let key = derive_mainline_key(&arid);
        assert_eq!(key.len(), 40, "Mainline key must be 40 hex characters");
        assert!(
            key.chars().all(|c| c.is_ascii_hexdigit()),
            "Key must be valid hex"
        );
    }

    #[test]
    fn test_different_salts() {
        let arid = ARID::new();
        let ipfs = derive_ipfs_key_name(&arid);
        let mainline = derive_mainline_key(&arid);
        assert_ne!(
            ipfs, mainline,
            "Different salts must produce different keys"
        );
    }
}
