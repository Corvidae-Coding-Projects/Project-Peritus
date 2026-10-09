//! Exact immutable observation metadata, distinct from a claim that authorization occurred.

use super::{ControlError, FileRange, FileSource, OperationId, Sha256Digest};
use crate::attachment::{MAX_FILE_BYTES, ValidatedFileText};
use peritus_types::ArtifactId;
use serde::Deserialize;
use serde::Serialize;

/// Metadata from one complete bounded scan and exact selected UTF-8 bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileObservation {
    source_digest: [u8; 32],
    source_bytes: u64,
    start: u64,
    end: u64,
    digest: [u8; 32],
}
impl FileObservation {
    /// Checks metadata bounds. The host must match these claims to the actual authorized read.
    ///
    /// # Errors
    /// Rejects impossible ranges or excessive selected/source bytes.
    pub fn new(
        source_digest: Sha256Digest,
        source_bytes: u64,
        range: (u64, u64),
        digest: Sha256Digest,
    ) -> Result<Self, ControlError> {
        let value = Self {
            source_digest: source_digest.into_bytes(),
            source_bytes,
            start: range.0,
            end: range.1,
            digest: digest.into_bytes(),
        };
        value.validate()?;
        Ok(value)
    }
    /// Returns digest of the complete source, including unselected bytes.
    #[must_use]
    pub const fn source_digest(self) -> Sha256Digest {
        Sha256Digest::new(self.source_digest)
    }
    /// Returns complete observed source size.
    #[must_use]
    pub const fn source_bytes(self) -> u64 {
        self.source_bytes
    }
    /// Returns exact resolved half-open selected byte interval.
    #[must_use]
    pub const fn range(self) -> (u64, u64) {
        (self.start, self.end)
    }
    /// Returns immutable selected byte digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        Sha256Digest::new(self.digest)
    }
    /// Returns exact selected size.
    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.end.saturating_sub(self.start)
    }
    /// Checks exact included bytes, without claiming to validate the unselected source.
    #[must_use]
    pub fn matches(self, text: &ValidatedFileText) -> bool {
        self.bytes() == text.bytes() && self.digest() == text.digest()
    }
    fn validate(self) -> Result<(), ControlError> {
        if self.source_bytes > super::source::MAX_SOURCE_BYTES
            || self.start > self.end
            || self.end > self.source_bytes
            || self.bytes() > MAX_FILE_BYTES
            || (self.start == 0
                && self.end == self.source_bytes
                && self.source_digest != self.digest)
        {
            return Err(ControlError::InvalidInput);
        }
        Ok(())
    }
}

/// One immutable version. Refresh adds a successor; it never rewrites an earlier version.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileVersion {
    operation: OperationId,
    artifact: [u8; 16],
    observation: FileObservation,
    consent_digest: [u8; 32],
}
impl FileVersion {
    /// Binds an observation to its own publication operation and exact preview/admission proof.
    ///
    /// # Errors
    /// Rejects structurally invalid observation metadata. This does not publish bytes or consent.
    pub fn new(
        operation: OperationId,
        artifact: ArtifactId,
        observation: FileObservation,
        consent_digest: Sha256Digest,
    ) -> Result<Self, ControlError> {
        observation.validate()?;
        Ok(Self {
            operation,
            artifact: artifact.into_bytes(),
            observation,
            consent_digest: consent_digest.into_bytes(),
        })
    }
    /// Returns this version's immutable publication operation.
    #[must_use]
    pub const fn operation(&self) -> OperationId {
        self.operation
    }
    /// Borrows the immutable source artifact identity.
    #[must_use]
    pub const fn artifact_bytes(&self) -> &[u8; 16] {
        &self.artifact
    }
    /// Returns exact observed source and selected-byte metadata.
    #[must_use]
    pub const fn observation(&self) -> FileObservation {
        self.observation
    }
    /// Returns the immutable consent or refresh-admission archive digest.
    #[must_use]
    pub const fn consent_digest(&self) -> Sha256Digest {
        Sha256Digest::new(self.consent_digest)
    }
    pub(super) fn validate(&self, source: &FileSource) -> Result<(), ControlError> {
        self.observation.validate()?;
        let (start, end) = self.observation.range();
        let range_matches = match source.range() {
            FileRange::All => start == 0 && end == self.observation.source_bytes(),
            FileRange::Bytes { start: expected_start, end: expected_end } => {
                start == expected_start && end == expected_end
            }
            FileRange::Lines { .. } => start < end,
        };
        if self.artifact == [0; 16] || !range_matches {
            return Err(ControlError::InvalidInput);
        }
        Ok(())
    }
}
