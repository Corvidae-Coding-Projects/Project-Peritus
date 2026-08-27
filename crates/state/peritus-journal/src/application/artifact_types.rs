//! Checked application-artifact catalog values.

use peritus_types::{ArtifactId, Sha256Digest};

use crate::{JournalError, JournalErrorKind};

/// Application artifact catalog state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApplicationArtifactState {
    /// Upload metadata exists but content is not yet published.
    Uploading,
    /// Immutable content and its producing journal position are available.
    Available,
    /// The attempted upload failed and may be inspected or replaced explicitly.
    Failed,
}

impl ApplicationArtifactState {
    pub(super) const fn from_tag(tag: i64) -> Option<Self> {
        match tag {
            1 => Some(Self::Uploading),
            2 => Some(Self::Available),
            3 => Some(Self::Failed),
            _ => None,
        }
    }
}

/// New bounded artifact catalog row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewApplicationArtifact {
    pub(super) artifact_id: ArtifactId,
    pub(super) digest: Sha256Digest,
    pub(super) byte_size: u64,
    pub(super) media_type: String,
}

impl NewApplicationArtifact {
    /// Creates pending artifact metadata.
    ///
    /// # Errors
    ///
    /// Returns invalid input for an empty, oversized, or non-ASCII media type.
    pub fn new(
        artifact_id: ArtifactId,
        digest: Sha256Digest,
        byte_size: u64,
        media_type: String,
    ) -> Result<Self, JournalError> {
        if media_type.is_empty() || media_type.len() > 255 || !media_type.is_ascii() {
            return Err(invalid("application artifact media type is invalid"));
        }
        if byte_size > i64::MAX as u64 {
            return Err(invalid("application artifact size exceeds SQLite range"));
        }
        Ok(Self { artifact_id, digest, byte_size, media_type })
    }
}

/// One durable application artifact catalog row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationArtifact {
    pub(super) artifact_id: ArtifactId,
    pub(super) digest: Sha256Digest,
    pub(super) byte_size: u64,
    pub(super) media_type: String,
    pub(super) state: ApplicationArtifactState,
    pub(super) producing_position: Option<u64>,
}

impl ApplicationArtifact {
    /// Returns the stable artifact identity.
    #[must_use]
    pub const fn artifact_id(&self) -> ArtifactId {
        self.artifact_id
    }

    /// Returns the immutable content digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Returns exact byte length.
    #[must_use]
    pub const fn byte_size(&self) -> u64 {
        self.byte_size
    }

    /// Borrows the media type.
    #[must_use]
    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    /// Returns catalog lifecycle state.
    #[must_use]
    pub const fn state(&self) -> ApplicationArtifactState {
        self.state
    }

    /// Returns the producing journal position once available.
    #[must_use]
    pub const fn producing_position(&self) -> Option<u64> {
        self.producing_position
    }
}

const fn invalid(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::InvalidInput, "validate application ledger value", detail)
}
