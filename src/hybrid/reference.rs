use bc_components::ARID;
use bc_envelope::prelude::*;

use super::error::Error;

/// Creates a reference envelope that points to content stored in IPFS.
///
/// Reference envelopes are small envelopes stored in the DHT that contain
/// a pointer to the actual envelope stored in IPFS. This allows the hybrid
/// storage layer to transparently handle large envelopes that exceed the
/// DHT size limit.
///
/// # Format
///
/// ```text
/// '' [
///     'dereferenceVia': "ipfs",
///     'id': <ARID>,
///     "size": <usize>
/// ]
/// ```
///
/// # Parameters
///
/// - `reference_arid`: The ARID used to look up the actual envelope in IPFS
/// - `actual_size`: Size of the actual envelope in bytes (for diagnostics)
///
/// # Returns
///
/// A reference envelope that can be stored in the DHT
pub fn create_reference_envelope(
    reference_arid: &ARID,
    actual_size: usize,
) -> Envelope {
    Envelope::unit()
        .add_assertion(known_values::DEREFERENCE_VIA, "ipfs")
        .add_assertion(known_values::ID, *reference_arid)
        .add_assertion("size", actual_size as i64)
}

/// Checks if an envelope is a reference envelope.
///
/// A reference envelope contains `dereferenceVia: "ipfs"` and an `id`
/// assertion.
///
/// # Parameters
///
/// - `envelope`: The envelope to check
///
/// # Returns
///
/// `true` if this is a reference envelope, `false` otherwise
pub fn is_reference_envelope(envelope: &Envelope) -> bool {
    // Check if subject is the unit value
    if !envelope.is_subject_unit() {
        return false;
    }

    // Check for dereferenceVia: "ipfs" assertion
    let has_dereference_via = envelope.assertions().iter().any(|assertion| {
        if let Ok(predicate) = assertion.try_predicate() {
            if let Some(kv) = predicate.as_known_value() {
                if kv.value() == known_values::DEREFERENCE_VIA_RAW {
                    if let Ok(object) = assertion.try_object() {
                        if let Ok(cbor) = object.subject().try_leaf() {
                            if let Ok(text) = cbor.try_into_text() {
                                return text == "ipfs";
                            }
                        }
                    }
                }
            }
        }
        false
    });

    if !has_dereference_via {
        return false;
    }

    // Check for id assertion

    envelope.assertions().iter().any(|assertion| {
        if let Ok(predicate) = assertion.try_predicate() {
            if let Some(kv) = predicate.as_known_value() {
                kv.value() == known_values::ID_RAW
            } else {
                false
            }
        } else {
            false
        }
    })
}

/// Extracts the reference ARID from a reference envelope.
///
/// # Parameters
///
/// - `envelope`: The reference envelope
///
/// # Returns
///
/// - `Ok(ARID)` if the reference ARID was successfully extracted
/// - `Err(HybridError)` if the envelope is not a reference or the ARID is
///   invalid
pub fn extract_reference_arid(envelope: &Envelope) -> Result<ARID, Error> {
    if !is_reference_envelope(envelope) {
        return Err(Error::NotReferenceEnvelope);
    }

    // Find the id assertion and extract the ARID
    for assertion in envelope.assertions() {
        if let Ok(predicate) = assertion.try_predicate() {
            if let Some(kv) = predicate.as_known_value() {
                if kv.value() == known_values::ID_RAW {
                    // The object's subject should be an ARID
                    if let Ok(object) = assertion.try_object() {
                        if let Ok(cbor) = object.subject().try_leaf() {
                            if let Ok(arid) = ARID::try_from(cbor.clone()) {
                                return Ok(arid);
                            } else {
                                return Err(Error::InvalidReferenceArid);
                            }
                        }
                    }
                }
            }
        }
    }

    Err(Error::NoIdAssertion)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_reference_envelope() {
        let reference_arid = ARID::new();
        let size = 5000;

        let envelope = create_reference_envelope(&reference_arid, size);

        // Check subject is unit
        assert!(envelope.is_subject_unit());

        // Should have 3 assertions
        assert_eq!(envelope.assertions().len(), 3);
    }

    #[test]
    fn test_is_reference_envelope() {
        let reference_arid = ARID::new();
        let size = 5000;

        let reference = create_reference_envelope(&reference_arid, size);
        assert!(is_reference_envelope(&reference));

        // Regular envelope should not be detected as reference
        let regular = Envelope::new("test data");
        assert!(!is_reference_envelope(&regular));

        // Envelope with wrong subject should not be detected
        let wrong_subject = Envelope::new("notunit")
            .add_assertion(known_values::DEREFERENCE_VIA, "ipfs")
            .add_assertion(known_values::ID, reference_arid);
        assert!(!is_reference_envelope(&wrong_subject));
    }

    #[test]
    fn test_extract_reference_arid() {
        let reference_arid = ARID::new();
        let size = 5000;

        let reference = create_reference_envelope(&reference_arid, size);
        let extracted = extract_reference_arid(&reference).unwrap();

        assert_eq!(extracted, reference_arid);
    }

    #[test]
    fn test_extract_reference_arid_from_non_reference() {
        let regular = Envelope::new("test data");
        let result = extract_reference_arid(&regular);

        assert!(result.is_err());
    }
}
