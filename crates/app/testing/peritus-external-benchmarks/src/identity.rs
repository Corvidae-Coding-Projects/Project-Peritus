//! Immutable identity for the native benchmark executable.

use std::{fs::File, io::Read as _};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::BenchmarkError;

const COMPILED_SOURCE_REVISION: Option<&str> = option_env!("PERITUS_SOURCE_REVISION");

/// Source and binary identity retained with every external benchmark invocation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BenchmarkAgentIdentity {
    /// Cargo package version compiled into the native agent.
    pub package_version: &'static str,
    /// Exact clean Git source revision supplied to the compiler.
    pub source_revision: &'static str,
    /// SHA-256 of the executable bytes used for this invocation.
    pub binary_sha256: String,
}

impl BenchmarkAgentIdentity {
    /// Inspects the current executable and returns its immutable benchmark identity.
    pub(crate) fn current() -> Result<Self, BenchmarkError> {
        let source_revision = required_source_revision(COMPILED_SOURCE_REVISION)?;
        let executable = std::env::current_exe().map_err(|error| {
            BenchmarkError::filesystem(
                "resolve current benchmark executable",
                "peritus-benchmark-agent",
                error,
            )
        })?;
        let mut file = File::open(&executable).map_err(|error| {
            BenchmarkError::filesystem("open current benchmark executable", &executable, error)
        })?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 8 * 1024];
        loop {
            let read = file.read(&mut buffer).map_err(|error| {
                BenchmarkError::filesystem("hash current benchmark executable", &executable, error)
            })?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok(Self {
            package_version: env!("CARGO_PKG_VERSION"),
            source_revision,
            binary_sha256: lowercase_hex(&hasher.finalize()),
        })
    }
}

fn required_source_revision(value: Option<&'static str>) -> Result<&'static str, BenchmarkError> {
    let revision = value.ok_or_else(|| {
        BenchmarkError::Identity(
            "PERITUS_SOURCE_REVISION was not supplied when the benchmark agent was built"
                .to_owned(),
        )
    })?;
    validate_source_revision(revision)?;
    Ok(revision)
}

fn lowercase_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn validate_source_revision(value: &str) -> Result<(), BenchmarkError> {
    if matches!(value.len(), 40 | 64)
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Ok(());
    }
    Err(BenchmarkError::Identity(
        "compiled source revision must be a full lowercase Git object ID".to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_full_sha1_and_sha256_revisions() {
        validate_source_revision("0123456789abcdef0123456789abcdef01234567").unwrap();
        validate_source_revision(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .unwrap();
    }

    #[test]
    fn requires_a_compiled_source_revision() {
        let error = required_source_revision(None).unwrap_err();

        assert!(error.to_string().contains("PERITUS_SOURCE_REVISION was not supplied"));
    }

    #[test]
    fn rejects_abbreviated_uppercase_and_nonhex_revisions() {
        for value in [
            "f8c928da",
            "F8C928DA4D4FAE3F5B6A4D4BC810CF6A2C75FF2C",
            "g8c928da4d4fae3f5b6a4d4bc810cf6a2c75ff2c",
        ] {
            assert!(validate_source_revision(value).is_err(), "accepted {value}");
        }
    }

    #[test]
    fn encodes_digest_bytes_as_lowercase_hex() {
        assert_eq!(lowercase_hex(&[0x00, 0x5a, 0xff]), "005aff");
    }
}
