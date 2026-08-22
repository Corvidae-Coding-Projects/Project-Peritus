//! Exact Git command observations and opaque object identifiers.

use super::{TempRepositoryError, TempRepositoryErrorKind};
use std::process::ExitStatus;

/// Full, untruncated observation of one Git subprocess.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitCommandOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl GitCommandOutput {
    pub(crate) const fn new(status: ExitStatus, stdout: Vec<u8>, stderr: Vec<u8>) -> Self {
        Self { status, stdout, stderr }
    }

    /// Returns the exact process exit status.
    #[must_use]
    pub const fn status(&self) -> ExitStatus {
        self.status
    }

    /// Returns whether the command exited successfully.
    #[must_use]
    pub fn success(&self) -> bool {
        self.status.success()
    }

    /// Returns complete standard output bytes.
    #[must_use]
    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }

    /// Returns complete standard error bytes.
    #[must_use]
    pub fn stderr(&self) -> &[u8] {
        &self.stderr
    }
}

/// An opaque lowercase hexadecimal Git object identifier.
///
/// Both SHA-1 and SHA-256 repositories are supported without assigning hash semantics here.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GitObjectId(String);

impl GitObjectId {
    pub(crate) fn new(value: String) -> Result<Self, TempRepositoryError> {
        let valid_length = matches!(value.len(), 40 | 64);
        if valid_length
            && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            Ok(Self(value))
        } else {
            Err(TempRepositoryError::new(
                TempRepositoryErrorKind::InvalidObjectId,
                format!("Git returned invalid object identifier {value:?}"),
            ))
        }
    }

    /// Borrows the exact lowercase hexadecimal representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
