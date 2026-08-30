//! Private file and executable retention for H3 evidence bundles.

use std::fs::{self, File};
use std::io::{Read as _, Write as _};
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use peritus_benchmarks::{ArtifactPath, EvidenceArtifact, QualificationError, Sha256Digest};
use sha2::{Digest as _, Sha256};

use crate::EvidenceError;

pub fn copy_executable(
    root: &Path,
    relative: &str,
    role: &'static str,
    source: &Path,
    expected: &Sha256Digest,
) -> Result<EvidenceArtifact, EvidenceError> {
    let metadata = fs::metadata(source)
        .map_err(|error| EvidenceError::io("inspect executable evidence", source, error))?;
    if !metadata.is_file() {
        return Err(EvidenceError::InvalidPath(source.to_path_buf()));
    }
    let destination = root.join(relative);
    create_private_parent(&destination)?;
    let mut input = File::open(source)
        .map_err(|error| EvidenceError::io("open executable evidence", source, error))?;
    let mut output = File::create(&destination)
        .map_err(|error| EvidenceError::io("create executable evidence", &destination, error))?;
    output
        .set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| EvidenceError::io("protect executable evidence", &destination, error))?;
    let (length, observed) = copy_and_digest(source, &destination, &mut input, &mut output)?;
    if &observed != expected {
        return Err(EvidenceError::ExecutableDigestMismatch {
            role,
            expected: expected.to_string(),
            observed: observed.to_string(),
        });
    }
    Ok(EvidenceArtifact::new(
        ArtifactPath::new(relative)?,
        "application/octet-stream",
        length,
        observed,
    )?)
}

pub fn write_private(path: &Path, bytes: &[u8]) -> Result<(), EvidenceError> {
    create_private_parent(path)?;
    let mut file = File::create(path)
        .map_err(|error| EvidenceError::io("create evidence artifact", path, error))?;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| EvidenceError::io("protect evidence artifact", path, error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| EvidenceError::io("write evidence artifact", path, error))
}

fn copy_and_digest(
    source: &Path,
    destination: &Path,
    input: &mut File,
    output: &mut File,
) -> Result<(u64, Sha256Digest), EvidenceError> {
    let mut hasher = Sha256::new();
    let mut length = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|error| EvidenceError::io("read executable evidence", source, error))?;
        if count == 0 {
            break;
        }
        output
            .write_all(&buffer[..count])
            .map_err(|error| EvidenceError::io("write executable evidence", destination, error))?;
        hasher.update(&buffer[..count]);
        length = length
            .checked_add(u64::try_from(count).unwrap_or(u64::MAX))
            .ok_or(QualificationError::ArithmeticOverflow("executable evidence length"))?;
    }
    output
        .sync_all()
        .map_err(|error| EvidenceError::io("sync executable evidence", destination, error))?;
    let digest = Sha256Digest::parse(lower_hex(&hasher.finalize()))?;
    Ok((length, digest))
}

fn create_private_parent(path: &Path) -> Result<(), EvidenceError> {
    let parent = path.parent().ok_or_else(|| EvidenceError::InvalidPath(path.to_path_buf()))?;
    fs::create_dir_all(parent)
        .map_err(|error| EvidenceError::io("create evidence directory", parent, error))?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
        .map_err(|error| EvidenceError::io("protect evidence directory", parent, error))
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_copy_is_private_and_content_bound() {
        let temporary = tempfile::tempdir().expect("temporary root");
        let source = temporary.path().join("runner");
        fs::write(&source, b"exact runner bytes").expect("source");
        let expected = Sha256Digest::of_bytes(b"exact runner bytes");

        let artifact =
            copy_executable(temporary.path(), "bundle/runner", "runner", &source, &expected)
                .expect("copy");

        let destination = temporary.path().join("bundle/runner");
        assert_eq!(artifact.digest(), &expected);
        assert_eq!(fs::read(&destination).expect("retained bytes"), b"exact runner bytes");
        assert_eq!(
            fs::metadata(destination).expect("metadata").permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn executable_copy_rejects_identity_drift() {
        let temporary = tempfile::tempdir().expect("temporary root");
        let source = temporary.path().join("subject");
        fs::write(&source, b"observed").expect("source");
        let expected = Sha256Digest::of_bytes(b"different");

        assert!(matches!(
            copy_executable(temporary.path(), "bundle/subject", "subject", &source, &expected,),
            Err(EvidenceError::ExecutableDigestMismatch { role: "subject", .. })
        ));
    }
}
