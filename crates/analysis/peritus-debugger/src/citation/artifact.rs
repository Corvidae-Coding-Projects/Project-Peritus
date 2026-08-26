//! Immutable citation values.

use crate::{
    DebuggerError, DebuggerErrorKind, DebuggerOperation, DebuggerRecovery, SelectionManifestId,
    SubjectId, TraceSelectionManifest,
};
use peritus_artifact_store::ArtifactDigest;
use peritus_types::{EventId, Sha256Digest};

/// Nonempty half-open byte range in a selected finalized ordinary artifact.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArtifactCitation {
    digest: ArtifactDigest,
    start: u64,
    end: u64,
}

impl ArtifactCitation {
    /// Creates a nonempty half-open range. Durable-size containment is checked with the manifest.
    ///
    /// # Errors
    ///
    /// Rejects empty or reversed ranges.
    pub fn new(digest: ArtifactDigest, start: u64, end: u64) -> Result<Self, DebuggerError> {
        if start >= end {
            Err(citation_error("artifact citation range must be nonempty and half-open"))
        } else {
            Ok(Self { digest, start, end })
        }
    }
    /// Returns the selected ordinary artifact digest.
    #[must_use]
    pub const fn digest(self) -> ArtifactDigest {
        self.digest
    }
    /// Returns the inclusive range start.
    #[must_use]
    pub const fn start(self) -> u64 {
        self.start
    }
    /// Returns the exclusive range end.
    #[must_use]
    pub const fn end(self) -> u64 {
        self.end
    }
}

/// Exact citation to one manifest-selected C7 event and optional ordinary artifact range.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvidenceCitation {
    manifest_id: SelectionManifestId,
    subject_id: SubjectId,
    event_id: EventId,
    journal_position: u64,
    frame_digest: Sha256Digest,
    artifact: Option<ArtifactCitation>,
}

impl EvidenceCitation {
    /// Validates and freezes a source citation against one immutable manifest.
    ///
    /// # Errors
    ///
    /// Rejects unselected events, any subject/position/frame mismatch, unrelated artifacts,
    /// vault references, and empty or out-of-range ordinary artifact ranges.
    pub fn new(
        manifest: &TraceSelectionManifest,
        subject_id: SubjectId,
        event_id: EventId,
        journal_position: u64,
        frame_digest: Sha256Digest,
        artifact: Option<ArtifactCitation>,
    ) -> Result<Self, DebuggerError> {
        let citation = Self {
            manifest_id: manifest.id(),
            subject_id,
            event_id,
            journal_position,
            frame_digest,
            artifact,
        };
        citation.validate_against(manifest)?;
        Ok(citation)
    }

    /// Revalidates all containment facts against the frozen manifest.
    ///
    /// # Errors
    ///
    /// Returns a citation error on any binding or range disagreement.
    pub fn validate_against(&self, manifest: &TraceSelectionManifest) -> Result<(), DebuggerError> {
        if self.manifest_id != manifest.id() {
            return Err(citation_error("citation names another selection manifest"));
        }
        let entry = manifest
            .event(self.event_id)
            .ok_or_else(|| citation_error("citation event is not selected"))?;
        if entry.subject().id() != self.subject_id
            || entry.journal_position() != self.journal_position
            || entry.frame_digest() != self.frame_digest
        {
            return Err(citation_error("citation subject, position, or frame digest differs"));
        }
        if let Some(range) = self.artifact {
            let selected = manifest.artifact(range.digest).ok_or_else(|| {
                citation_error("citation artifact is not selected ordinary evidence")
            })?;
            if selected.source_event().is_some_and(|source| source != self.event_id)
                || range.end > selected.size()
            {
                return Err(citation_error("artifact range or source-event ownership differs"));
            }
        }
        Ok(())
    }

    /// Returns the manifest identity.
    #[must_use]
    pub const fn manifest_id(&self) -> SelectionManifestId {
        self.manifest_id
    }
    /// Returns the exact subject identity.
    #[must_use]
    pub const fn subject_id(&self) -> SubjectId {
        self.subject_id
    }
    /// Returns the C7/C0 event identity.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Returns the one-based C0 position.
    #[must_use]
    pub const fn journal_position(&self) -> u64 {
        self.journal_position
    }
    /// Returns the exact frame digest.
    #[must_use]
    pub const fn frame_digest(&self) -> Sha256Digest {
        self.frame_digest
    }
    /// Returns an optional finalized ordinary-artifact range.
    #[must_use]
    pub const fn artifact(&self) -> Option<ArtifactCitation> {
        self.artifact
    }

    pub(crate) fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.extend_from_slice(self.manifest_id.as_bytes());
        bytes.extend_from_slice(self.subject_id.as_bytes());
        bytes.extend_from_slice(self.event_id.as_bytes());
        bytes.extend_from_slice(&self.journal_position.to_be_bytes());
        bytes.extend_from_slice(self.frame_digest.as_bytes());
        bytes.push(u8::from(self.artifact.is_some()));
        if let Some(artifact) = self.artifact {
            bytes.extend_from_slice(artifact.digest.as_bytes());
            bytes.extend_from_slice(&artifact.start.to_be_bytes());
            bytes.extend_from_slice(&artifact.end.to_be_bytes());
        }
    }
}

fn citation_error(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Citation,
        DebuggerOperation::ValidateCitation,
        DebuggerRecovery::CorrectInput,
        detail,
    )
}
