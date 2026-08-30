//! Streaming content identity for operator-selected executable artifacts.

use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read as _};
use std::path::Path;

use peritus_benchmarks::Sha256Digest;
use sha2::{Digest as _, Sha256};

/// Computes one file's SHA-256 without retaining its contents in memory.
///
/// # Errors
///
/// Returns the underlying filesystem error when the file cannot be opened or read.
#[must_use = "the digest must be checked or retained"]
pub fn sha256_file(path: &Path) -> io::Result<Sha256Digest> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let mut encoded = String::with_capacity(64);
    for byte in hasher.finalize() {
        write!(encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    Sha256Digest::parse(encoded).map_err(|_| io::Error::other("internal SHA-256 encoding failed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streamed_digest_matches_in_memory_contract() {
        let temporary = tempfile::NamedTempFile::new().expect("temporary");
        std::fs::write(temporary.path(), b"peritus-h3").expect("write");
        assert_eq!(
            sha256_file(temporary.path()).expect("digest"),
            Sha256Digest::of_bytes(b"peritus-h3")
        );
    }
}
