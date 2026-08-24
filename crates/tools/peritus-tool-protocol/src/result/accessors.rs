//! Read-only terminal-result projections and canonical encoding.

use super::{ToolResult, ToolTiming, TruncationMetadata, wire};
use crate::{
    ArtifactReference, BoundedJson, BoundedText, ReplayIdentity, SchemaDigest, ToolFailure,
};
use peritus_types::{ActionId, Sha256Digest};

impl ToolResult {
    /// Returns the action identity.
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }
    /// Returns the descriptor digest.
    #[must_use]
    pub const fn descriptor_digest(&self) -> SchemaDigest {
        self.descriptor_digest
    }
    /// Returns the prepared-call digest.
    #[must_use]
    pub const fn prepared_digest(&self) -> Sha256Digest {
        self.prepared_digest
    }
    /// Returns replay identity.
    #[must_use]
    pub const fn replay_identity(&self) -> ReplayIdentity {
        self.replay_identity
    }
    /// Returns the closed status.
    #[must_use]
    pub const fn status(&self) -> super::ResultStatus {
        self.status
    }
    /// Borrows structured output.
    #[must_use]
    pub const fn structured(&self) -> Option<&BoundedJson> {
        self.structured.as_ref()
    }
    /// Borrows typed failure.
    #[must_use]
    pub const fn failure_value(&self) -> Option<&ToolFailure> {
        self.failure.as_ref()
    }
    /// Borrows human rendering.
    #[must_use]
    pub const fn human_rendering(&self) -> &BoundedText {
        &self.human_rendering
    }
    /// Borrows model rendering.
    #[must_use]
    pub const fn model_rendering(&self) -> &BoundedText {
        &self.model_rendering
    }
    /// Borrows artifact references.
    #[must_use]
    pub fn artifacts(&self) -> &[ArtifactReference] {
        &self.artifacts
    }
    /// Returns timing.
    #[must_use]
    pub const fn timing(&self) -> ToolTiming {
        self.timing
    }
    /// Returns truncation truth.
    #[must_use]
    pub const fn truncation(&self) -> TruncationMetadata {
        self.truncation
    }
    /// Returns the observed progress count.
    #[must_use]
    pub const fn progress_count(&self) -> u32 {
        self.progress_count
    }

    /// Returns stable version-one canonical terminal-result bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        wire::result(self)
    }
}
