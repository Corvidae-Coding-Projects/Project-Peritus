//! Additive revisioned workbench controls. DTO validation never authenticates an actor.

use crate::{AppErrorCode, AppProtocolError, ControlOperationId, ConversationId};
use peritus_types::{Sha256Digest, WorkspaceId};

mod inputs;
pub use inputs::*;
mod library;
pub use library::*;
mod brief;
pub use brief::*;
mod context;
pub use context::*;
mod compaction;
pub use compaction::*;
mod execution;
pub use execution::WorkbenchExecutionSettings;
mod goal;
pub use goal::*;
mod images;
pub use images::*;
mod files;
pub use files::*;
mod review;
pub use review::*;
mod launch;
pub use launch::*;
mod checkpoints;
pub use checkpoints::*;
mod permissions;
pub use permissions::*;
mod memory;
pub use memory::*;
mod init;
pub use init::*;

/// Maximum bytes in a user-selected library title.
pub const MAX_CONVERSATION_TITLE_BYTES: usize = 256;

/// Checked exact library title; never interpreted as an instruction.
#[derive(Clone, Eq, PartialEq)]
pub struct ConversationTitle(String);

impl ConversationTitle {
    /// Validates a nonempty inert bounded title.
    ///
    /// # Errors
    /// Rejects empty, oversized or terminal-control-containing text.
    pub fn new(value: String) -> Result<Self, AppProtocolError> {
        if value.trim().is_empty()
            || value.len() > MAX_CONVERSATION_TITLE_BYTES
            || value.chars().any(char::is_control)
        {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        Ok(Self(value))
    }
    /// Borrows exact title bytes as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for ConversationTitle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConversationTitle").field("bytes", &self.0.len()).finish_non_exhaustive()
    }
}

mod intent;
pub use intent::WorkbenchIntent;

/// A selected conversation in an exact workspace; actor identity comes only from the daemon.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchQuery {
    conversation: ConversationId,
    workspace: WorkspaceId,
}
impl WorkbenchQuery {
    /// Binds both independent identities explicitly.
    #[must_use]
    pub const fn new(conversation: ConversationId, workspace: WorkspaceId) -> Self {
        Self { conversation, workspace }
    }
    /// Returns user conversation identity.
    #[must_use]
    pub const fn conversation(self) -> ConversationId {
        self.conversation
    }
    /// Returns workspace identity; resolution must not fall back to another workspace.
    #[must_use]
    pub const fn workspace(self) -> WorkspaceId {
        self.workspace
    }
}

/// Idempotent exact revision mutation. A socket acknowledgement is not acceptance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchCommand {
    operation: ControlOperationId,
    query: WorkbenchQuery,
    expected_revision: u64,
    intent: WorkbenchIntent,
}
impl WorkbenchCommand {
    /// Creates a typed intent without making any effect or authority claim.
    #[must_use]
    pub const fn new(
        operation: ControlOperationId,
        query: WorkbenchQuery,
        expected_revision: u64,
        intent: WorkbenchIntent,
    ) -> Self {
        Self { operation, query, expected_revision, intent }
    }
    /// Returns the reconnect-stable operation identity.
    #[must_use]
    pub const fn operation(&self) -> ControlOperationId {
        self.operation
    }
    /// Returns exact conversation/workspace scope.
    #[must_use]
    pub const fn query(&self) -> WorkbenchQuery {
        self.query
    }
    /// Returns inspected aggregate revision; zero means absent.
    #[must_use]
    pub const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }
    /// Borrows the closed effect intent.
    #[must_use]
    pub const fn intent(&self) -> &WorkbenchIntent {
        &self.intent
    }
}

/// Bounded revisioned metadata projection; opening it never starts work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchSnapshot {
    query: WorkbenchQuery,
    revision: u64,
    title: ConversationTitle,
    pinned: bool,
    archived: bool,
}
impl WorkbenchSnapshot {
    /// Validates the positive committed revision.
    ///
    /// # Errors
    /// Rejects the absent-state revision zero.
    pub fn new(
        query: WorkbenchQuery,
        revision: u64,
        title: ConversationTitle,
        pinned: bool,
        archived: bool,
    ) -> Result<Self, AppProtocolError> {
        if revision == 0 {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        Ok(Self { query, revision, title, pinned, archived })
    }
    /// Returns exact snapshot scope.
    #[must_use]
    pub const fn query(&self) -> WorkbenchQuery {
        self.query
    }
    /// Returns the committed aggregate revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    /// Borrows the exact title.
    #[must_use]
    pub const fn title(&self) -> &ConversationTitle {
        &self.title
    }
    /// Returns library pin state.
    #[must_use]
    pub const fn pinned(&self) -> bool {
        self.pinned
    }
    /// Returns library archive state.
    #[must_use]
    pub const fn archived(&self) -> bool {
        self.archived
    }
}

/// Host-published exact receipt. A client cannot use this DTO as an authority token.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchReceipt {
    operation: ControlOperationId,
    query: WorkbenchQuery,
    accepted_revision: u64,
    payload_digest: Sha256Digest,
}
impl WorkbenchReceipt {
    /// Validates the positive revision of the originally accepted operation.
    ///
    /// # Errors
    /// Rejects absent-state revision zero.
    pub const fn new(
        operation: ControlOperationId,
        query: WorkbenchQuery,
        accepted_revision: u64,
        payload_digest: Sha256Digest,
    ) -> Result<Self, AppProtocolError> {
        if accepted_revision == 0 {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        Ok(Self { operation, query, accepted_revision, payload_digest })
    }
    /// Returns the exact operation identity acknowledged durably.
    #[must_use]
    pub const fn operation(&self) -> ControlOperationId {
        self.operation
    }
    /// Returns original conversation/workspace scope.
    #[must_use]
    pub const fn query(&self) -> WorkbenchQuery {
        self.query
    }
    /// Returns original acceptance revision even if later state has changed.
    #[must_use]
    pub const fn accepted_revision(&self) -> u64 {
        self.accepted_revision
    }
    /// Returns digest of the exact actor-bound canonical host operation.
    #[must_use]
    pub const fn payload_digest(&self) -> Sha256Digest {
        self.payload_digest
    }
}
