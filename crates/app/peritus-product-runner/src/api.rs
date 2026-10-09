//! Explicit public facade; implementation modules retain their existing ownership.

pub use crate::attachment_read::{AttachmentReadRequest, AttachmentReadResponse};
#[cfg(not(verus_only))]
pub use crate::budget::ProductRunProgress;
pub use crate::context_config::{
    LocalCompactorSandbox, LocalContextConfig, LocalContextEngine, LocalProcessConfig,
    LocalSemanticBackend,
};
pub use crate::conversation_mode::ConversationMode;
#[cfg(not(verus_only))]
pub use crate::developer_tools::{
    CommandRuntime, FolderPatchAuthority, FolderPatchAuthorityPlan, PreviewTerminal,
    UncertainEffect, UncertainEffectState, acknowledge_uncertain_effect, checked_protected_file,
    uncertain_effects,
};
pub use crate::developer_tools::{
    FolderPatchAuthorityPlanRequest, PreviewCommand, PreviewLaunch, PreviewObservation,
    PreviewOutputMatch, PreviewOutputMatchSource, PreviewOutputRange, PreviewOutputStream,
    PreviewOwner, PreviewProcessState,
};
pub use crate::discard_recovery::DiscardTransactionState;
pub use crate::error::{ProductRunnerError, ProductRunnerErrorKind};
#[cfg(not(verus_only))]
pub use crate::execution::{
    ConversationView, ProductDeliveryScope, ProductRunInput, ProductRunOutcome, ProductRunOutput,
    ProductRunPhase, ProductRunQuestion, ProductRunResume, ProductRunUpdate, ProductRunner,
    RoleProviders, RunObserver, WorkspaceMutationKind,
};
#[cfg(not(verus_only))]
pub use crate::local_context::inspect_local_context;
#[cfg(verus_only)]
pub use crate::verified_api::{
    CommandRuntime, ConversationView, FolderPatchAuthority, FolderPatchAuthorityPlan,
    PreviewTerminal, ProductDeliveryScope, ProductRunInput, ProductRunOutcome, ProductRunOutput,
    ProductRunPhase, ProductRunProgress, ProductRunQuestion, ProductRunResume, ProductRunUpdate,
    ProductRunner, RoleProviders, RunObserver, UncertainEffect, UncertainEffectState,
    WorkspaceMutationKind, acknowledge_uncertain_effect, checked_protected_file, uncertain_effects,
};
pub use crate::workspace_kind::ProductWorkspaceKind;
