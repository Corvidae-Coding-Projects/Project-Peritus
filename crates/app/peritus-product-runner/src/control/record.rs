//! Revisioned conversation metadata and exact idempotent intents.

use super::{CONTROL_SCHEMA, ControlError, ConversationId, OperationId};
use peritus_codec::sha256;
use peritus_types::{ActorId, WorkspaceId};
use serde::Deserialize;
use serde::Serialize;
mod checkpoint_codec;
mod codec;
mod intent;
mod projection;
mod replay;
mod seed;
use codec::{decode, encode};
pub use intent::ControlIntent;
pub use replay::ConversationReplay;
pub use seed::ConversationSeed;

/// Exact authenticated operation binding; the daemon supplies actor/workspace identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlOperation {
    schema: u16,
    id: OperationId,
    conversation: ConversationId,
    actor: [u8; 16],
    workspace: [u8; 16],
    expected_revision: u64,
    intent: ControlIntent,
}

impl ControlOperation {
    /// Constructs one exact expected-revision operation, without accepting or executing it.
    #[must_use]
    pub const fn new(
        id: OperationId,
        conversation: ConversationId,
        actor: ActorId,
        workspace: WorkspaceId,
        expected_revision: u64,
        intent: ControlIntent,
    ) -> Self {
        Self {
            schema: CONTROL_SCHEMA,
            id,
            conversation,
            actor: actor.into_bytes(),
            workspace: workspace.into_bytes(),
            expected_revision,
            intent,
        }
    }
    /// Returns its reconnect-stable operation identity.
    #[must_use]
    pub const fn id(&self) -> OperationId {
        self.id
    }
    /// Returns the selected conversation identity.
    #[must_use]
    pub const fn conversation(&self) -> ConversationId {
        self.conversation
    }
    /// Returns exact authenticated actor bytes.
    #[must_use]
    pub const fn actor_bytes(&self) -> &[u8; 16] {
        &self.actor
    }
    /// Returns exact workspace bytes.
    #[must_use]
    pub const fn workspace_bytes(&self) -> &[u8; 16] {
        &self.workspace
    }
    /// Returns the aggregate revision the user inspected; zero means absent.
    #[must_use]
    pub const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }
    /// A pause only narrows the named goal; advancing accounting must not prevent it.
    const fn accepts_revision(&self, revision: u64) -> bool {
        revision == self.expected_revision
            || (matches!(self.intent, ControlIntent::PauseGoal { .. })
                && self.expected_revision > 0
                && self.expected_revision < revision)
    }
    /// Borrows the typed intent, not executable prompt text.
    #[must_use]
    pub const fn intent(&self) -> &ControlIntent {
        &self.intent
    }
    /// Encodes validated deterministic bytes used to bind the idempotency receipt.
    ///
    /// # Errors
    /// Rejects unsupported schemas, reserved identities, or a bound/encoding failure.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ControlError> {
        self.validate()?;
        checkpoint_codec::encode_operation(self)
    }
    /// Decodes exact immutable operation bytes, with checkpoint metadata separate from the core.
    ///
    /// # Errors
    /// Rejects malformed, unsupported, noncanonical payloads or an oversized control core.
    pub fn parse(bytes: &[u8]) -> Result<Self, ControlError> {
        let value: Self = serde_json::from_slice(bytes).map_err(|_| ControlError::InvalidInput)?;
        if value.canonical_bytes()? != bytes {
            return Err(ControlError::InvalidInput);
        }
        Ok(value)
    }
    fn validate(&self) -> Result<(), ControlError> {
        if self.schema != CONTROL_SCHEMA {
            return Err(ControlError::UnsupportedSchema);
        }
        if self.actor == [0; 16] || self.workspace == [0; 16] {
            return Err(ControlError::InvalidInput);
        }
        match &self.intent {
            ControlIntent::ReserveFork { branch, .. }
            | ControlIntent::ReserveAutomaticFork { branch, .. }
            | ControlIntent::PublishRestoreBranch { branch, .. }
                if branch.operation() == self.id
                    && branch.source() == self.conversation
                    && branch.source_workspace_bytes() == &self.workspace => {}
            ControlIntent::CreateFork { branch }
                if branch.operation() == self.id
                    && branch.child() == self.conversation
                    && branch.child_workspace_bytes() == &self.workspace => {}
            ControlIntent::ReserveFork { .. }
            | ControlIntent::ReserveAutomaticFork { .. }
            | ControlIntent::CreateFork { .. }
            | ControlIntent::PublishRestoreBranch { .. } => {
                return Err(ControlError::InvalidInput);
            }
            _ => {}
        }
        Ok(())
    }
}

/// Current public metadata; checkpoint manifests are independent of the bounded control core.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationRecord {
    schema: u16,
    minimum_reader: u16,
    id: ConversationId,
    owner: [u8; 16],
    workspace: [u8; 16],
    revision: u64,
    title: super::ControlTitle,
    pinned: bool,
    archived: bool,
    #[serde(default, skip_serializing_if = "super::InputLedger::is_empty")]
    inputs: super::InputLedger,
    #[serde(default, skip_serializing_if = "super::TaskBrief::is_empty")]
    brief: super::TaskBrief,
    #[serde(default, skip_serializing_if = "super::ContextSelections::is_empty")]
    context: super::ContextSelections,
    #[serde(default, skip_serializing_if = "super::PromptView::is_empty")]
    prompt_view: super::PromptView,
    #[serde(default, skip_serializing_if = "super::ImageAttachments::is_empty")]
    images: super::ImageAttachments,
    #[serde(default, skip_serializing_if = "super::FileAttachments::is_empty")]
    files: super::FileAttachments,
    #[serde(default, skip_serializing_if = "super::ReviewLedger::is_empty")]
    reviews: super::ReviewLedger,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    execution: Option<ControlExecution>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    goal: Option<super::GoalRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    replies: Vec<super::PublicReplyReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    checkpoints: Vec<super::UserCheckpoint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    restores: Vec<super::RestoreOperation>,
}

/// Immutable initial execution binding. Later attempts share this governed conversation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlExecution {
    run: [u8; 16],
    start_operation: OperationId,
    settings_digest: [u8; 32],
}
impl ControlExecution {
    /// Returns the original run identity bytes.
    #[must_use]
    pub const fn run_bytes(&self) -> &[u8; 16] {
        &self.run
    }
    /// Returns the exact operation that admitted execution.
    #[must_use]
    pub const fn start_operation(&self) -> OperationId {
        self.start_operation
    }
    /// Returns the original role/model/mode selection digest.
    #[must_use]
    pub const fn settings_digest(&self) -> peritus_types::Sha256Digest {
        peritus_types::Sha256Digest::new(self.settings_digest)
    }
}

mod receipt;
mod transition;

/// A proposed receipt becomes accepted only when published with the state in one C0 transaction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlReceipt {
    schema: u16,
    operation: OperationId,
    conversation: ConversationId,
    accepted_revision: u64,
    payload_digest: [u8; 32],
}
