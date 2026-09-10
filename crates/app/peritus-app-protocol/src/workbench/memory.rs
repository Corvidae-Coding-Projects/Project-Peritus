//! Explicit project-local guidance, exact provenance and deterministic future-request rendering.

use crate::{
    AppErrorCode, AppProtocolError, ControlOperationId, ConversationId, WorkbenchInvocationId,
    WorkbenchQuery,
};
use peritus_types::{Sha256Digest, WorkspaceId};

mod render;
#[cfg(test)]
mod tests;
pub use render::*;
mod transition;
pub use transition::*;

/// Maximum exact UTF-8 bytes in one saved guidance revision.
pub const MAX_WORKBENCH_GUIDANCE_BYTES: usize = 8 * 1024;
/// Maximum exact UTF-8 bytes in a forget reason.
pub const MAX_WORKBENCH_GUIDANCE_REASON_BYTES: usize = 1024;
/// Maximum rows in one guidance inspection page or one future-request rendering.
pub const MAX_WORKBENCH_GUIDANCE_PAGE: usize = 32;
/// Maximum active and forgotten guidance identities in one project.
pub const MAX_WORKBENCH_GUIDANCE_RECORDS: usize = 4096;
/// Maximum deterministic project-guidance bytes admitted to one future request.
pub const MAX_WORKBENCH_GUIDANCE_RENDER_BYTES: usize = 64 * 1024;

const fn invalid() -> AppProtocolError {
    AppProtocolError::new(AppErrorCode::MalformedFrame, None)
}

const fn stale() -> AppProtocolError {
    AppProtocolError::new(AppErrorCode::StaleRevision, None)
}

const fn capacity() -> AppProtocolError {
    AppProtocolError::new(AppErrorCode::LimitExceeded, None)
}

fn valid_text(text: &str, max: usize) -> bool {
    !text.trim().is_empty()
        && text.len() <= max
        && !text
            .chars()
            .any(|character| character.is_control() && character != '\n' && character != '\t')
}

/// Exact inert user-approved guidance text.
#[derive(Clone, Eq, PartialEq)]
pub struct WorkbenchGuidanceText(String);

impl WorkbenchGuidanceText {
    /// Validates bounded text without interpreting commands or granting authority.
    ///
    /// # Errors
    /// Rejects empty, oversized, or terminal-control-bearing text.
    pub fn new(text: String) -> Result<Self, AppProtocolError> {
        if !valid_text(&text, MAX_WORKBENCH_GUIDANCE_BYTES) {
            return Err(invalid());
        }
        Ok(Self(text))
    }

    /// Borrows the exact accepted UTF-8 text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for WorkbenchGuidanceText {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorkbenchGuidanceText")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Exact bounded explanation recorded with a guidance tombstone.
#[derive(Clone, Eq, PartialEq)]
pub struct WorkbenchGuidanceReason(String);

impl WorkbenchGuidanceReason {
    /// Validates an inert, display-safe reason.
    ///
    /// # Errors
    /// Rejects empty, oversized, or terminal-control-bearing text.
    pub fn new(reason: String) -> Result<Self, AppProtocolError> {
        if !valid_text(&reason, MAX_WORKBENCH_GUIDANCE_REASON_BYTES) {
            return Err(invalid());
        }
        Ok(Self(reason))
    }

    /// Borrows the exact reason.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for WorkbenchGuidanceReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorkbenchGuidanceReason")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Project-first scope. V1 has no cross-workspace scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchGuidanceScope {
    /// Every conversation in the record's exact workspace.
    Project,
    /// Only one explicitly selected conversation in that workspace.
    Conversation(ConversationId),
}

/// Original source of guidance that an authenticated user explicitly approved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchGuidanceSource {
    /// Text authored directly by the user in the memory form.
    UserAuthored,
    /// Exact immutable public agent reply that the user separately accepted.
    AcceptedPublicReply {
        /// Reply publication operation.
        operation: ControlOperationId,
        /// Invocation immediately preceding the public reply.
        invocation: WorkbenchInvocationId,
        /// SHA-256 of the exact accepted public text.
        digest: Sha256Digest,
    },
}

/// Public lifecycle without pretending forgotten content was erased from immutable history.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchGuidanceLifecycle {
    /// Eligible for scope-filtered future retrieval.
    Active,
    /// Excluded by a durable tombstone.
    Forgotten,
}

/// Stable project-local guidance identity and workspace binding.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct WorkbenchGuidanceIdentity {
    id: ControlOperationId,
    workspace: WorkspaceId,
}

impl WorkbenchGuidanceIdentity {
    /// Binds the initial save identity to exactly one workspace.
    #[must_use]
    pub const fn new(id: ControlOperationId, workspace: WorkspaceId) -> Self {
        Self { id, workspace }
    }

    /// Returns the initial save operation used as the stable identity.
    #[must_use]
    pub const fn id(self) -> ControlOperationId {
        self.id
    }

    /// Returns the only workspace in which this guidance is eligible.
    #[must_use]
    pub const fn workspace(self) -> WorkspaceId {
        self.workspace
    }
}

/// Monotonic content and dependent-view revisions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchGuidanceVersion {
    record: u64,
    dependency: u64,
}

impl WorkbenchGuidanceVersion {
    /// Validates positive durable revisions.
    ///
    /// # Errors
    /// Rejects zero revisions.
    pub const fn new(record: u64, dependency: u64) -> Result<Self, AppProtocolError> {
        if record == 0 || dependency == 0 {
            return Err(invalid());
        }
        Ok(Self { record, dependency })
    }

    /// Returns the selected guidance-record revision.
    #[must_use]
    pub const fn record(self) -> u64 {
        self.record
    }

    /// Returns the workspace guidance dependency revision.
    #[must_use]
    pub const fn dependency(self) -> u64 {
        self.dependency
    }
}

/// Exact text, source provenance, and project-local eligibility scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchGuidanceContent {
    text: WorkbenchGuidanceText,
    source: WorkbenchGuidanceSource,
    scope: WorkbenchGuidanceScope,
}

impl WorkbenchGuidanceContent {
    /// Validates that accepted agent-reply provenance matches the exact text.
    ///
    /// # Errors
    /// Rejects an accepted reply whose retained digest differs from the supplied text.
    pub fn new(
        text: WorkbenchGuidanceText,
        source: WorkbenchGuidanceSource,
        scope: WorkbenchGuidanceScope,
    ) -> Result<Self, AppProtocolError> {
        let digest = peritus_codec::sha256(text.as_str().as_bytes());
        if matches!(source, WorkbenchGuidanceSource::AcceptedPublicReply { digest: source, .. } if source != digest)
        {
            return Err(invalid());
        }
        Ok(Self { text, source, scope })
    }

    /// Borrows the exact user-approved text.
    #[must_use]
    pub const fn text(&self) -> &WorkbenchGuidanceText {
        &self.text
    }

    /// Returns the original source provenance.
    #[must_use]
    pub const fn source(&self) -> WorkbenchGuidanceSource {
        self.source
    }

    /// Returns project-wide or selected-conversation eligibility.
    #[must_use]
    pub const fn scope(&self) -> WorkbenchGuidanceScope {
        self.scope
    }

    /// Returns SHA-256 over exact text bytes.
    #[must_use]
    pub fn digest(&self) -> Sha256Digest {
        peritus_codec::sha256(self.text.as_str().as_bytes())
    }
}

/// Most recent explicit user validation of the current content revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchGuidanceValidation {
    operation: ControlOperationId,
    content_digest: Sha256Digest,
}

impl WorkbenchGuidanceValidation {
    /// Records the exact approving operation and content digest.
    #[must_use]
    pub const fn new(operation: ControlOperationId, content_digest: Sha256Digest) -> Self {
        Self { operation, content_digest }
    }

    /// Returns the authenticated user operation that validated this content.
    #[must_use]
    pub const fn operation(self) -> ControlOperationId {
        self.operation
    }

    /// Returns the digest of the content that was validated.
    #[must_use]
    pub const fn content_digest(self) -> Sha256Digest {
        self.content_digest
    }
}

mod lifecycle;
pub use lifecycle::*;
mod inspection;
pub use inspection::*;
