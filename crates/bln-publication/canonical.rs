use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{Code, Diagnostic, Result, Stage};

/// Validate serialized storage before copying a semantic payload. Counts bytes
/// without allocating another representation of the input.
pub(crate) fn encoded_size(value: &impl Serialize, limit: usize) -> Result<usize> {
    struct Counter {
        bytes: usize,
        limit: usize,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes) {
                return Err(std::io::Error::other(
                    "serialized input byte limit exceeded",
                ));
            }
            self.bytes += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter { bytes: 0, limit };
    serde_json::to_writer(&mut counter, value).map_err(|_| {
        Diagnostic::new(
            Code::ResourceLimit,
            Stage::Construction,
            "serialized input byte limit exceeded",
        )
    })?;
    Ok(counter.bytes)
}

/// UTF-8 JSON: recursively sorted map keys, ordered arrays, no floats and no
/// implicit Unicode normalization. Property types further restrict their domain.
pub fn canonical_bytes(value: &impl Serialize) -> Result<Vec<u8>> {
    fn check(value: &Value) -> Result<()> {
        match value {
            Value::Number(n) if n.is_f64() => Err(Diagnostic::new(
                Code::InvalidProperty,
                Stage::Construction,
                "floating-point values are not part of publication schema v1",
            )),
            Value::Array(values) => values.iter().try_for_each(check),
            Value::Object(values) => values.values().try_for_each(check),
            _ => Ok(()),
        }
    }
    let mut value = serde_json::to_value(value).map_err(encoding_error)?;
    check(&value)?;
    value.sort_all_objects();
    serde_json::to_vec(&value).map_err(encoding_error)
}

fn encoding_error(error: serde_json::Error) -> Diagnostic {
    Diagnostic::new(
        Code::InvalidProperty,
        Stage::Construction,
        error.to_string(),
    )
}

/// Domain separators are length-delimited; schema version is part of the domain.
pub fn fingerprint(domain: &str, value: &impl Serialize) -> Result<String> {
    let mut hash = Sha256::new();
    hash.update(b"berlin-publication\0");
    hash.update((domain.len() as u64).to_be_bytes());
    hash.update(domain.as_bytes());
    hash.update(canonical_bytes(value)?);
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Content digest for captured input bytes, distinct from semantic fingerprints.
pub fn input_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
