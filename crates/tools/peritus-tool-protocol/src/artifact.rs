//! Content-addressed artifact references without filesystem paths.

use crate::{BoundedText, ProtocolError, ProtocolErrorKind};
use peritus_types::{ActionId, Sha256Digest};

/// Whether the artifact contains the complete claimed byte stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactCompleteness {
    /// The exact complete bytes are published.
    Complete,
    /// Publication is intentionally truncated and labelled.
    Truncated,
    /// Publication failed or completeness cannot be established.
    Indeterminate,
}

/// Exact invocation provenance bound into an artifact reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArtifactProvenance {
    action_id: ActionId,
    prepared_digest: Sha256Digest,
}

impl ArtifactProvenance {
    /// Creates invocation-bound provenance.
    #[must_use]
    pub const fn new(action_id: ActionId, prepared_digest: Sha256Digest) -> Self {
        Self { action_id, prepared_digest }
    }
    /// Returns the producing action.
    #[must_use]
    pub const fn action_id(self) -> ActionId {
        self.action_id
    }
    /// Returns the producing prepared-call digest.
    #[must_use]
    pub const fn prepared_digest(self) -> Sha256Digest {
        self.prepared_digest
    }
}

/// One bounded, content-addressed output artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactReference {
    digest: Sha256Digest,
    size: u64,
    media_type: BoundedText,
    label: BoundedText,
    completeness: ArtifactCompleteness,
    provenance: ArtifactProvenance,
}

impl ArtifactReference {
    /// Creates a nonempty artifact reference.
    ///
    /// # Errors
    ///
    /// Rejects zero-byte artifacts; empty output belongs in the structured result.
    pub fn new(
        digest: Sha256Digest,
        size: u64,
        media_type: BoundedText,
        label: BoundedText,
        completeness: ArtifactCompleteness,
        provenance: ArtifactProvenance,
    ) -> Result<Self, ProtocolError> {
        if size == 0 {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidEnvelope,
                "artifact.size",
                "artifact size must be nonzero",
            ));
        }
        Ok(Self { digest, size, media_type, label, completeness, provenance })
    }

    /// Returns the content digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Returns the declared byte size.
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }
    /// Borrows the media type.
    #[must_use]
    pub const fn media_type(&self) -> &BoundedText {
        &self.media_type
    }
    /// Borrows the safe display label.
    #[must_use]
    pub const fn label(&self) -> &BoundedText {
        &self.label
    }
    /// Returns completeness.
    #[must_use]
    pub const fn completeness(&self) -> ArtifactCompleteness {
        self.completeness
    }
    /// Returns invocation provenance.
    #[must_use]
    pub const fn provenance(&self) -> ArtifactProvenance {
        self.provenance
    }

    /// Returns stable version-one canonical artifact-reference bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = crate::wire::begin(5);
        bytes.extend_from_slice(self.digest.as_bytes());
        crate::wire::u64_value(&mut bytes, self.size);
        crate::wire::text(&mut bytes, self.media_type.as_str());
        crate::wire::text(&mut bytes, self.label.as_str());
        bytes.push(match self.completeness {
            ArtifactCompleteness::Complete => 1,
            ArtifactCompleteness::Truncated => 2,
            ArtifactCompleteness::Indeterminate => 3,
        });
        bytes.extend_from_slice(self.provenance.action_id.as_bytes());
        bytes.extend_from_slice(self.provenance.prepared_digest.as_bytes());
        bytes
    }
}
