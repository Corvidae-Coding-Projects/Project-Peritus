//! Explicit public facade; implementation modules retain their existing ownership.

pub use crate::attachment_read::{AttachmentReadRequest, AttachmentReadResponse};

pub use crate::budget::ProductRunProgress;
pub use crate::context_config::{
    CandidateObservationPolicy, DecisionViewPolicy, LocalCompactorSandbox, LocalContextConfig,
    LocalContextEngine, LocalProcessConfig, LocalSemanticBackend, ReadOnlyPlanExecution,
};
pub use crate::conversation_mode::ConversationMode;

pub use crate::developer_tools::{
    CommandRuntime, FolderPatchAuthority, FolderPatchAuthorityPlan,
    MaterializationAuthorityContext, MaterializationAuthorityRequest,
    OwnedMaterializationAuthority, OwnedRestrictedProcessAuthority, PreviewTerminal,
    RestrictedProcessContext, UncertainEffect, UncertainEffectState, acknowledge_uncertain_effect,
    checked_protected_file, uncertain_effects,
};
pub use crate::developer_tools::{
    FolderPatchAuthorityPlanRequest, PreviewCommand, PreviewLaunch, PreviewObservation,
    PreviewOutputMatch, PreviewOutputMatchSource, PreviewOutputRange, PreviewOutputStream,
    PreviewOwner, PreviewProcessState,
};
pub use crate::discard_recovery::DiscardTransactionState;
pub use crate::error::{ProductRunnerError, ProductRunnerErrorKind};

pub use crate::execution::{
    ConversationView, NativeModuleJobConfiguration, ProductDeliveryScope, ProductRunInput,
    ProductRunOutcome, ProductRunOutput, ProductRunPhase, ProductRunQuestion, ProductRunResume,
    ProductRunUpdate, ProductRunner, RoleProviders, RunObserver, WorkspaceMutationKind,
};

pub use crate::local_context::{LocalContextHandle, MemoryTools, inspect_local_context};

pub use crate::guided_design::GUIDED_DESIGN_IMPLEMENT_REQUEST;
pub use crate::workspace_kind::ProductWorkspaceKind;
