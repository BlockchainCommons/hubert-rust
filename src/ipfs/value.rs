use std::io::Cursor;

use bc_envelope::Envelope;
use dcbor::CBOR;
use futures_util::TryStreamExt;
use ipfs_api_backend_hyper::{IpfsApi, IpfsClient};

use super::error::{GetError, PutError};

/// Serialize an envelope to dCBOR bytes.
pub fn serialize_envelope(envelope: &Envelope) -> Result<Vec<u8>, PutError> {
    let cbor: CBOR = envelope.clone().into();
    Ok(cbor.to_cbor_data())
}

/// Deserialize dCBOR bytes to an envelope.
pub fn deserialize_envelope(bytes: &[u8]) -> Result<Envelope, GetError> {
    let cbor = CBOR::try_from_data(bytes)?;
    Ok(Envelope::try_from(cbor)?)
}

/// Add (upload) bytes to IPFS and return the CID.
pub async fn add_bytes(
    client: &IpfsClient,
    bytes: Vec<u8>,
) -> Result<String, PutError> {
    let add_res = client
        .add(Cursor::new(bytes))
        .await
        .map_err(|e| PutError::DaemonError(e.to_string()))?;
    Ok(add_res.hash)
}

/// Cat (download) bytes from IPFS by CID.
pub async fn cat_bytes(
    client: &IpfsClient,
    cid: &str,
) -> Result<Vec<u8>, GetError> {
    let mut stream = client.cat(cid);
    let mut result = Vec::new();
    while let Some(chunk) = stream
        .try_next()
        .await
        .map_err(|e| GetError::DaemonError(e.to_string()))?
    {
        result.extend_from_slice(&chunk);
    }
    Ok(result)
}

/// Pin a CID to ensure it persists in local IPFS storage.
pub async fn pin_cid(
    client: &IpfsClient,
    cid: &str,
    recursive: bool,
) -> Result<(), PutError> {
    client
        .pin_add(cid, recursive)
        .await
        .map_err(|e| PutError::DaemonError(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_envelope_roundtrip() {
        let original = Envelope::new("test data");
        let bytes = serialize_envelope(&original).expect("serialize failed");
        let roundtrip =
            deserialize_envelope(&bytes).expect("deserialize failed");
        assert_eq!(original, roundtrip);
    }

    #[test]
    fn test_envelope_with_assertions() {
        let original = Envelope::new("subject")
            .add_assertion("key1", "value1")
            .add_assertion("key2", 42);
        let bytes = serialize_envelope(&original).expect("serialize failed");
        let roundtrip =
            deserialize_envelope(&bytes).expect("deserialize failed");
        assert_eq!(original, roundtrip);
    }
}
