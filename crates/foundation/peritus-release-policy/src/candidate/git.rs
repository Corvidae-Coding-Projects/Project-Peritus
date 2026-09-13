//! Checked Git commit identities.

use crate::{ConstructionError, ConstructionErrorKind};
use vstd::prelude::*;

verus! {

/// Git object hash format.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GitObjectFormat {
    /// SHA-1 Git object identity.
    Sha1,
    /// SHA-256 Git object identity.
    Sha256,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum GitCommitBytes {
    Sha1([u8; 20]),
    Sha256([u8; 32]),
}

/// Checked exact Git commit identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GitCommitId {
    bytes: GitCommitBytes,
}

impl GitCommitId {
    /// Exact admission predicate for a SHA-1 commit identity.
    pub open spec fn sha1_inputs_valid(bytes: [u8; 20]) -> bool {
        exists |index: int| 0 <= index < 20 && bytes[index] != 0
    }

    /// Exact admission predicate for a SHA-256 commit identity.
    pub open spec fn sha256_inputs_valid(bytes: [u8; 32]) -> bool {
        exists |index: int| 0 <= index < 32 && bytes[index] != 0
    }

    /// Creates a nonzero SHA-1 commit identity.
    ///
    /// # Errors
    ///
    /// Returns [`ConstructionErrorKind::ZeroIdentity`] for an all-zero identifier.
    pub const fn sha1(
        bytes: [u8; 20],
    ) -> (result: Result<Self, ConstructionError>)
        ensures
            result.is_ok() == Self::sha1_inputs_valid(bytes),
            match result {
                Ok(value) => value.spec_format() == GitObjectFormat::Sha1
                    && value.spec_sha1_bytes() == Some(bytes)
                    && value.spec_sha256_bytes().is_none(),
                Err(error) => error.spec_kind() == ConstructionErrorKind::ZeroIdentity,
            },
    {
        let mut nonzero = false;
        let mut index = 0;
        while index < bytes.len()
            invariant
                0 <= index <= bytes.len(),
                nonzero == (exists |prior: int| 0 <= prior < index && bytes[prior] != 0),
            decreases bytes.len() - index,
        {
            if bytes[index] != 0 { nonzero = true; }
            index += 1;
        }
        if nonzero {
            Ok(Self { bytes: GitCommitBytes::Sha1(bytes) })
        } else {
            Err(ConstructionError::new(ConstructionErrorKind::ZeroIdentity))
        }
    }

    /// Creates a nonzero SHA-256 commit identity.
    ///
    /// # Errors
    ///
    /// Returns [`ConstructionErrorKind::ZeroIdentity`] for an all-zero identifier.
    pub const fn sha256(
        bytes: [u8; 32],
    ) -> (result: Result<Self, ConstructionError>)
        ensures
            result.is_ok() == Self::sha256_inputs_valid(bytes),
            match result {
                Ok(value) => value.spec_format() == GitObjectFormat::Sha256
                    && value.spec_sha1_bytes().is_none()
                    && value.spec_sha256_bytes() == Some(bytes),
                Err(error) => error.spec_kind() == ConstructionErrorKind::ZeroIdentity,
            },
    {
        if crate::validation::digest_bytes_nonzero(&bytes) {
            Ok(Self { bytes: GitCommitBytes::Sha256(bytes) })
        } else {
            Err(ConstructionError::new(ConstructionErrorKind::ZeroIdentity))
        }
    }

    /// Returns the Git object hash format.
    #[must_use]
    pub const fn format(&self) -> (format: GitObjectFormat)
        ensures format == self.spec_format()
    {
        match self.bytes {
            GitCommitBytes::Sha1(_) => GitObjectFormat::Sha1,
            GitCommitBytes::Sha256(_) => GitObjectFormat::Sha256,
        }
    }

    /// Returns SHA-1 bytes when this is a SHA-1 repository commit.
    #[must_use]
    pub const fn sha1_bytes(&self) -> (bytes: Option<[u8; 20]>)
        ensures bytes == self.spec_sha1_bytes()
    {
        match self.bytes {
            GitCommitBytes::Sha1(bytes) => Some(bytes),
            GitCommitBytes::Sha256(_) => None,
        }
    }

    /// Returns SHA-256 bytes when this is a SHA-256 repository commit.
    #[must_use]
    pub const fn sha256_bytes(&self) -> (bytes: Option<[u8; 32]>)
        ensures bytes == self.spec_sha256_bytes()
    {
        match self.bytes {
            GitCommitBytes::Sha1(_) => None,
            GitCommitBytes::Sha256(bytes) => Some(bytes),
        }
    }

    /// Logical view of the Git object format.
    pub closed spec fn spec_format(&self) -> GitObjectFormat {
        match self.bytes {
            GitCommitBytes::Sha1(_) => GitObjectFormat::Sha1,
            GitCommitBytes::Sha256(_) => GitObjectFormat::Sha256,
        }
    }

    /// Logical view of SHA-1 bytes, when present.
    pub closed spec fn spec_sha1_bytes(&self) -> Option<[u8; 20]> {
        match self.bytes {
            GitCommitBytes::Sha1(bytes) => Some(bytes),
            GitCommitBytes::Sha256(_) => None,
        }
    }

    /// Logical view of SHA-256 bytes, when present.
    pub closed spec fn spec_sha256_bytes(&self) -> Option<[u8; 32]> {
        match self.bytes {
            GitCommitBytes::Sha1(_) => None,
            GitCommitBytes::Sha256(bytes) => Some(bytes),
        }
    }
}

} // verus!
