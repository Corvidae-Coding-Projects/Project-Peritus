//! Checked Git commit identities.

use crate::{ConstructionError, ConstructionErrorKind};

#[doc = " Git object hash format."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GitObjectFormat {
    #[doc = " SHA-1 Git object identity."]
    Sha1,
    #[doc = " SHA-256 Git object identity."]
    Sha256,
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum GitCommitBytes {
    Sha1([u8; 20]),
    Sha256([u8; 32]),
}
#[doc = " Checked exact Git commit identity."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GitCommitId {
    bytes: GitCommitBytes,
}
impl GitCommitId {
    #[doc = " Creates a nonzero SHA-1 commit identity."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`ConstructionErrorKind::ZeroIdentity`] for an all-zero identifier."]
    pub const fn sha1(bytes: [u8; 20]) -> Result<Self, ConstructionError> {
        let mut nonzero = false;
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                nonzero = true;
            }
            index += 1;
        }
        if nonzero {
            Ok(Self { bytes: GitCommitBytes::Sha1(bytes) })
        } else {
            Err(ConstructionError::new(ConstructionErrorKind::ZeroIdentity))
        }
    }
    #[doc = " Creates a nonzero SHA-256 commit identity."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`ConstructionErrorKind::ZeroIdentity`] for an all-zero identifier."]
    pub const fn sha256(bytes: [u8; 32]) -> Result<Self, ConstructionError> {
        if crate::validation::digest_bytes_nonzero(&bytes) {
            Ok(Self { bytes: GitCommitBytes::Sha256(bytes) })
        } else {
            Err(ConstructionError::new(ConstructionErrorKind::ZeroIdentity))
        }
    }
    #[doc = " Returns the Git object hash format."]
    #[must_use]
    pub const fn format(&self) -> GitObjectFormat {
        match self.bytes {
            GitCommitBytes::Sha1(_) => GitObjectFormat::Sha1,
            GitCommitBytes::Sha256(_) => GitObjectFormat::Sha256,
        }
    }
    #[doc = " Returns SHA-1 bytes when this is a SHA-1 repository commit."]
    #[must_use]
    pub const fn sha1_bytes(&self) -> Option<[u8; 20]> {
        match self.bytes {
            GitCommitBytes::Sha1(bytes) => Some(bytes),
            GitCommitBytes::Sha256(_) => None,
        }
    }
    #[doc = " Returns SHA-256 bytes when this is a SHA-256 repository commit."]
    #[must_use]
    pub const fn sha256_bytes(&self) -> Option<[u8; 32]> {
        match self.bytes {
            GitCommitBytes::Sha1(_) => None,
            GitCommitBytes::Sha256(bytes) => Some(bytes),
        }
    }
}
