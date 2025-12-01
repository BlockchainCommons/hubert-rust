use bc_components::ARID;
use bc_crypto::hkdf_hmac_sha256;
use chacha20::{
    ChaCha20,
    cipher::{KeyIvInit, StreamCipher},
};

/// Derive a deterministic key from an ARID using a specific salt.
///
/// Uses HKDF to derive key material from the ARID, ensuring that:
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
/// Derived key bytes
pub fn derive_key(salt: &[u8], arid: &ARID, output_len: usize) -> Vec<u8> {
    let arid_bytes = arid.data();
    hkdf_hmac_sha256(salt, arid_bytes, output_len)
}

/// Derive an IPNS key name from an ARID.
///
/// Returns a 64-character hex string suitable for use as an IPFS key name.
pub fn derive_ipfs_key_name(arid: &ARID) -> String {
    const SALT: &[u8] = b"hubert-ipfs-ipns-v1";
    hex::encode(derive_key(SALT, arid, 32))
}

/// Derive Mainline DHT key material from an ARID.
///
/// Returns 20 bytes of key material (SHA-1 compatible length).
pub fn derive_mainline_key(arid: &ARID) -> Vec<u8> {
    const SALT: &[u8] = b"hubert-mainline-dht-v1";
    derive_key(SALT, arid, 20)
}

/// Obfuscate or deobfuscate data using ChaCha20 with an ARID-derived key.
///
/// This function uses ChaCha20 as a stream cipher to XOR the data with a
/// keystream derived from the ARID. Since XOR is symmetric, the same function
/// is used for both obfuscation and deobfuscation.
///
/// The result appears as uniform random data to anyone who doesn't have the
/// ARID, hiding both the structure and content of the reference envelope.
///
/// # Parameters
///
/// - `arid`: The ARID used to derive the obfuscation key
/// - `data`: The data to obfuscate or deobfuscate
///
/// # Returns
///
/// The obfuscated (or deobfuscated) data
pub fn obfuscate_with_arid(arid: &ARID, data: impl AsRef<[u8]>) -> Vec<u8> {
    const SALT: &[u8] = b"hubert-obfuscation-v1";

    let data = data.as_ref();
    if data.is_empty() {
        return data.to_vec();
    }

    // Derive a 32-byte key from the ARID using HKDF with domain-specific salt
    let key: [u8; 32] = hkdf_hmac_sha256(SALT, arid.data(), 32)
        .try_into()
        .expect("HKDF produces exactly 32 bytes");

    // Derive IV from the key (last 12 bytes, reversed)
    let iv: [u8; 12] = key
        .iter()
        .rev()
        .take(12)
        .copied()
        .collect::<Vec<u8>>()
        .try_into()
        .expect("12 bytes for IV");

    let mut cipher = ChaCha20::new(&key.into(), &iv.into());
    let mut buffer = data.to_vec();
    cipher.apply_keystream(&mut buffer);
    buffer
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
        assert_eq!(key.len(), 20, "Mainline key must be 20 bytes");
    }

    #[test]
    fn test_different_salts() {
        let arid = ARID::new();
        let ipfs = derive_ipfs_key_name(&arid);
        let mainline = hex::encode(derive_mainline_key(&arid));
        assert_ne!(
            ipfs, mainline,
            "Different salts must produce different keys"
        );
    }

    #[test]
    fn test_obfuscation_roundtrip() {
        let arid = ARID::new();
        let original = b"Hello, this is test data for obfuscation!";

        let obfuscated = obfuscate_with_arid(&arid, original);
        let deobfuscated = obfuscate_with_arid(&arid, &obfuscated);

        assert_eq!(original.as_slice(), deobfuscated.as_slice());
    }

    #[test]
    fn test_obfuscation_produces_different_output() {
        let arid = ARID::new();
        let original = b"Test data";

        let obfuscated = obfuscate_with_arid(&arid, original);

        assert_ne!(original.as_slice(), obfuscated.as_slice());
    }

    #[test]
    fn test_obfuscation_deterministic() {
        let arid = ARID::new();
        let data = b"Same data twice";

        let obfuscated1 = obfuscate_with_arid(&arid, data);
        let obfuscated2 = obfuscate_with_arid(&arid, data);

        assert_eq!(obfuscated1, obfuscated2);
    }

    #[test]
    fn test_obfuscation_different_arids() {
        let arid1 = ARID::new();
        let arid2 = ARID::new();
        let data = b"Same data, different keys";

        let obfuscated1 = obfuscate_with_arid(&arid1, data);
        let obfuscated2 = obfuscate_with_arid(&arid2, data);

        assert_ne!(obfuscated1, obfuscated2);
    }

    #[test]
    fn test_obfuscation_empty_data() {
        let arid = ARID::new();
        let empty: &[u8] = &[];

        let obfuscated = obfuscate_with_arid(&arid, empty);

        assert!(obfuscated.is_empty());
    }
}
