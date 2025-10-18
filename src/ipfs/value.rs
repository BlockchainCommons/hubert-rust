use std::io::Cursor;

use futures_util::TryStreamExt;
use ipfs_api_backend_hyper::{IpfsApi, IpfsClient};

use super::error::Error;

/// Add (upload) bytes to IPFS and return the CID.
pub async fn add_bytes(
    client: &IpfsClient,
    bytes: Vec<u8>,
) -> Result<String, Error> {
    let add_res = client.add(Cursor::new(bytes)).await?;
    Ok(add_res.hash)
}

/// Cat (download) bytes from IPFS by CID.
pub async fn cat_bytes(
    client: &IpfsClient,
    cid: &str,
) -> Result<Vec<u8>, Error> {
    let mut stream = client.cat(cid);
    let mut result = Vec::new();
    while let Some(chunk) = stream.try_next().await? {
        result.extend_from_slice(&chunk);
    }
    Ok(result)
}

/// Pin a CID to ensure it persists in local IPFS storage.
pub async fn pin_cid(
    client: &IpfsClient,
    cid: &str,
    recursive: bool,
) -> Result<(), Error> {
    client.pin_add(cid, recursive).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use bc_envelope::Envelope;
    use dcbor::CBOREncodable;

    #[test]
    fn test_envelope_roundtrip() {
        let original = Envelope::new("test data");
        let bytes = original.to_cbor_data();
        let roundtrip =
            Envelope::try_from_cbor_data(bytes).expect("deserialize failed");
        assert_eq!(original, roundtrip);
    }

    #[test]
    fn test_envelope_with_assertions() {
        let original = Envelope::new("subject")
            .add_assertion("key1", "value1")
            .add_assertion("key2", 42);
        let bytes = original.to_cbor_data();
        let roundtrip =
            Envelope::try_from_cbor_data(bytes).expect("deserialize failed");
        assert_eq!(original, roundtrip);
    }
}
