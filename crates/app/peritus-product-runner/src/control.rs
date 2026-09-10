//! Pure durable workbench records and admission decisions, independent of A3 and presentation.
//!
//! A returned transition is a proposal until the daemon commits it through C0. These records
//! contain no reusable approvals, credentials, process handles, or granted workspace authority.

mod branch;
mod brief;
mod checkpoint;
mod compaction;
mod context;
mod files;
mod guidance;
mod identity;
pub use files::{
    FileAttachment, FileAttachments, FileMode, FileObservation, FileRange, FileSelection,
    FileSource, FileVersion,
};
pub use guidance::{
    GuidanceAudit, GuidanceContent, GuidanceScope, GuidanceSelection, GuidanceSource,
};
mod images;
pub use images::{ImageAttachment, ImageAttachments, ImageFormat, ImageMetadata, ImageSelection};
mod goal;
pub use goal::{
    GoalAdmission, GoalBudget, GoalCriterion, GoalCriterionKind, GoalCriterionState, GoalPauseMode,
    GoalRecord, GoalRole, GoalRoleUsage, GoalSettlement, GoalState, GoalUsage, GoalUsageReport,
};
mod inputs;
mod permissions;
mod record;
mod reply;
mod review;
#[cfg(test)]
mod tests;
mod text;

pub use branch::{
    ChildBudgetAllocation, ChildBudgetReservation, ConversationBranch, ConversationBranchMode,
};
pub use brief::{BriefBinding, BriefField, TaskBrief};
pub use checkpoint::{
    CheckpointFileMode, CheckpointFileVersion, CheckpointPath, CheckpointReferences,
    MAX_CHECKPOINT_PATHS, MAX_CHECKPOINTS, MAX_RESTORES, RestoreOperation, RestoreStatus,
    UserCheckpoint,
};
pub use compaction::{CompactedReply, PromptView};
pub use context::{ContextPreference, ContextSelection, ContextSelections, ContextTarget};
pub use identity::{CheckpointId, ConversationId, InputId, InvocationId, OperationId, RestoreId};
pub use inputs::{
    InputCapture, InputLedger, InputRevision, InputSelection, InputState, InvocationInputs,
    QueueIntent,
};
pub use permissions::{HostPermissions, PermissionCapability, PermissionPolicy};
pub use record::{
    ControlExecution, ControlIntent, ControlOperation, ControlReceipt, ConversationRecord,
    ConversationSeed,
};
pub use reply::PublicReplyReference;
pub use review::{
    ReviewAnchor, ReviewComment, ReviewCommentState, ReviewFeedback, ReviewLedger, ReviewRange,
    ReviewTarget,
};
pub use text::{ControlText, ControlTextIter};

/// Maximum exact bytes in one control operation or current-state projection.
pub const MAX_CONTROL_BYTES: usize = 1024 * 1024;
/// Initial explicitly versioned control-state generation; not the legacy run JSON schema.
pub const CONTROL_SCHEMA: u16 = 1;

/// Stable pure-control rejection with no embedded private input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlError {
    /// An identity, payload, or bounded field is invalid.
    InvalidInput,
    /// A caller read an older aggregate revision.
    StaleRevision,
    /// An operation identity was reused for different input.
    IdempotencyConflict,
    /// The caller is not the recorded owner or selected workspace differs.
    ScopeMismatch,
    /// A configured count, byte, or revision bound would be exceeded.
    Capacity,
    /// The persisted generation is not supported by this reader.
    UnsupportedSchema,
    /// The selected conversation does not exist.
    NotFound,
}

impl std::fmt::Display for ControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidInput => "invalid bounded control input",
            Self::StaleRevision => "control state changed; refresh and reapply the exact intent",
            Self::IdempotencyConflict => "operation identity already binds different content",
            Self::ScopeMismatch => "control operation does not match its owner or workspace",
            Self::Capacity => "control state capacity exceeded",
            Self::UnsupportedSchema => "control state requires a supported reader generation",
            Self::NotFound => "selected control conversation was not found",
        })
    }
}
impl std::error::Error for ControlError {}
