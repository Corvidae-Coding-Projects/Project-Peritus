//! Deterministic SHA-256 artifact identities.

use std::{fmt, fs::File, io::Read as _, path::Path};

use sha2::{Digest as _, Sha256};

use crate::{QualificationError, QualificationErrorCode, QualificationRecovery};

const DIGEST_BYTES: usize = 32;
const HEX_BYTES: usize = DIGEST_BYTES * 2;

/// Exact SHA-256 digest used by H2 manifests and evidence.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Sha256Digest([u8; DIGEST_BYTES]);

impl Sha256Digest {
    /// Creates a digest from exact bytes.
    #[must_use]
    pub const fn new(bytes: [u8; DIGEST_BYTES]) -> Self {
        Self(bytes)
    }

    /// Parses exactly 64 lowercase or uppercase hexadecimal digits.
    ///
    /// # Errors
    ///
    /// Returns invalid input when length or characters are not a SHA-256 representation.
    pub fn from_hex(value: &str) -> Result<Self, QualificationError> {
        if value.len() != HEX_BYTES || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(invalid_digest("SHA-256 must contain exactly 64 hexadecimal digits"));
        }
        let mut bytes = [0_u8; DIGEST_BYTES];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = (nibble(pair[0]) << 4) | nibble(pair[1]);
        }
        Ok(Self(bytes))
    }

    /// Returns the exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; DIGEST_BYTES] {
        &self.0
    }

    /// Renders lowercase hexadecimal without allocation ambiguity.
    #[must_use]
    pub fn to_hex(self) -> String {
        let mut output = String::with_capacity(HEX_BYTES);
        for byte in self.0 {
            use fmt::Write as _;
            write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
        }
        output
    }
}

impl fmt::Debug for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Sha256Digest").field(&self.to_hex()).finish()
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

/// Size and digest of one exact release artifact.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArtifactDigest {
    byte_length: u64,
    sha256: Sha256Digest,
}

impl ArtifactDigest {
    /// Creates a digest observation.
    #[must_use]
    pub const fn new(byte_length: u64, sha256: Sha256Digest) -> Self {
        Self { byte_length, sha256 }
    }

    /// Returns the exact artifact size.
    #[must_use]
    pub const fn byte_length(self) -> u64 {
        self.byte_length
    }

    /// Returns the SHA-256 identity.
    #[must_use]
    pub const fn sha256(self) -> Sha256Digest {
        self.sha256
    }
}

/// Hashes in-memory bytes.
#[must_use]
pub fn digest_bytes(bytes: &[u8]) -> ArtifactDigest {
    let digest: [u8; DIGEST_BYTES] = Sha256::digest(bytes).into();
    ArtifactDigest::new(bytes.len() as u64, Sha256Digest::new(digest))
}

/// Hashes a regular file while enforcing an artifact-size bound.
///
/// # Errors
///
/// Returns typed file or bound failures and rejects non-regular files.
pub fn digest_file(
    path: impl AsRef<Path>,
    maximum_bytes: u64,
) -> Result<ArtifactDigest, QualificationError> {
    let path = path.as_ref();
    let metadata = path.metadata().map_err(|_| file_error("inspect package artifact"))?;
    if !metadata.is_file() {
        return Err(file_error("package artifact is not a regular file"));
    }
    if metadata.len() > maximum_bytes {
        return Err(QualificationError::new(
            QualificationErrorCode::EvidenceBound,
            QualificationRecovery::RebuildRelease,
            "hash package artifact",
            "artifact exceeds the configured byte bound",
        ));
    }
    let mut file = File::open(path).map_err(|_| file_error("open package artifact"))?;
    let mut hasher = Sha256::new();
    let mut observed = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|_| file_error("read package artifact"))?;
        if read == 0 {
            break;
        }
        observed = observed.checked_add(read as u64).ok_or_else(|| {
            QualificationError::new(
                QualificationErrorCode::EvidenceBound,
                QualificationRecovery::RebuildRelease,
                "hash package artifact",
                "artifact byte count overflowed",
            )
        })?;
        if observed > maximum_bytes {
            return Err(QualificationError::new(
                QualificationErrorCode::EvidenceBound,
                QualificationRecovery::RebuildRelease,
                "hash package artifact",
                "artifact changed beyond the configured byte bound while reading",
            ));
        }
        hasher.update(&buffer[..read]);
    }
    if observed != metadata.len() {
        return Err(QualificationError::new(
            QualificationErrorCode::Integrity,
            QualificationRecovery::RebuildRelease,
            "hash package artifact",
            "artifact size changed while it was being hashed",
        ));
    }
    Ok(ArtifactDigest::new(observed, Sha256Digest::new(hasher.finalize().into())))
}

const fn nibble(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        b'A'..=b'F' => value - b'A' + 10,
        _ => 0,
    }
}

fn invalid_digest(detail: &'static str) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::InvalidInput,
        QualificationRecovery::CorrectInput,
        "parse SHA-256 digest",
        detail,
    )
}

fn file_error(detail: &'static str) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::FileObservation,
        QualificationRecovery::RebuildRelease,
        "observe package artifact",
        detail,
    )
}
