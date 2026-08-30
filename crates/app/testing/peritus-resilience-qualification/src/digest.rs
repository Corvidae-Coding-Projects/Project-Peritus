//! Streaming release-candidate digest calculation.

use std::fs::File;
use std::io::Read as _;
use std::path::Path;

use peritus_resilience::EvidenceDigest;
use sha2::{Digest as _, Sha256};

pub fn file(path: &Path) -> Result<EvidenceDigest, std::io::Error> {
    let mut input = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let bytes = input.read(&mut buffer)?;
        if bytes == 0 {
            return Ok(EvidenceDigest::from_bytes(hasher.finalize().into()));
        }
        hasher.update(&buffer[..bytes]);
    }
}

pub fn hex(digest: EvidenceDigest) -> String {
    let mut output = String::with_capacity(64);
    for byte in digest.as_bytes() {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}
