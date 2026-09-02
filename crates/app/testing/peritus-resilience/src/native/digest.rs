//! Streaming SHA-256 helpers for native H1 identities and artifacts.

use std::fs::File;
use std::io::Read as _;
use std::path::Path;

use sha2::{Digest as _, Sha256};

use crate::EvidenceDigest;

use super::config::NativeAdapterError;

pub(super) fn file(path: &Path) -> Result<EvidenceDigest, NativeAdapterError> {
    let mut file = File::open(path)
        .map_err(|error| NativeAdapterError::filesystem("open digest input", path, error))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| NativeAdapterError::filesystem("read digest input", path, error))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(EvidenceDigest::from_bytes(hasher.finalize().into()))
}

pub(super) fn bytes(value: &[u8]) -> EvidenceDigest {
    EvidenceDigest::from_bytes(Sha256::digest(value).into())
}

pub(super) fn hex(value: EvidenceDigest) -> String {
    use std::fmt::Write as _;

    value.as_bytes().iter().fold(String::with_capacity(64), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    })
}
