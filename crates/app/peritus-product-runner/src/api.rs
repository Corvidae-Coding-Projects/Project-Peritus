//! Explicit public facade; implementation modules retain their existing ownership.

#[cfg(not(verus_only))]
pub use crate::budget::{
    ProductRunProgress, ResourceIoErrorKind, ResourceMeasurement, ResourceMeasurementStatus,
    ResourceObservationCause, ResourceObservationCoverage, ResourceObservationOperation,
};
pub use crate::context_config::{
    LocalCompactorSandbox, LocalContextConfig, LocalContextEngine, LocalProcessConfig,
    LocalSemanticBackend,
};
pub use crate::conversation_mode::ConversationMode;
pub use crate::context_sources::{
    ContextSource, ContextSourceKind, ContextSourcePage, ContextSourceSlice,
    MAX_CONTEXT_SOURCE_PAGE, MAX_CONTEXT_SOURCE_SLICE_BYTES,
};
#[cfg(not(verus_only))]
pub use crate::developer_tools::{
    CommandRuntime, FolderPatchAuthority, FolderPatchAuthorityPlan, PreviewTerminal,
    ManagedGateNetworkCatalog, ManagedGateNetworkDestination, ManagedGateNetworkGrant,
    UncertainEffect, UncertainEffectState, acknowledge_uncertain_effect,
    checked_protected_file, uncertain_effects,
};
pub use crate::developer_tools::{
    FolderPatchAuthorityPlanRequest, PreviewCommand, PreviewLaunch, PreviewObservation,
    PreviewProcessState,
};
pub use crate::discard_recovery::DiscardTransactionState;
pub use crate::error::{ProductRunnerError, ProductRunnerErrorKind, ProductRunnerFailureCause};
#[cfg(not(verus_only))]
pub use crate::execution::{
    ConversationView, ProductDeliveryScope, ProductRunInput, ProductRunOutcome, ProductRunOutput,
    ProductRunPhase, ProductRunQuestion, ProductRunResume, ProductRunUpdate, ProductRunner,
    RoleProviders, RunObserver, WorkspaceCheckpointFuture, WorkspaceMutationKind,
};
#[cfg(not(verus_only))]
pub use crate::local_context::{LocalContextInspection, inspect_local_context, open_local_context_inspection};
#[cfg(verus_only)]
pub use crate::verified_api::{
    CommandRuntime, ConversationView, FolderPatchAuthority, FolderPatchAuthorityPlan,
    PreviewTerminal, ProductDeliveryScope, ProductRunInput, ProductRunOutcome, ProductRunOutput,
    ProductRunPhase, ProductRunProgress, ProductRunQuestion, ProductRunResume, ProductRunUpdate,
    ProductRunner, ResourceIoErrorKind, ResourceMeasurement, ResourceMeasurementStatus,
    ResourceObservationCause, ResourceObservationCoverage, ResourceObservationOperation,
    RoleProviders, RunObserver, UncertainEffect, UncertainEffectState, WorkspaceCheckpointFuture,
    WorkspaceMutationKind, acknowledge_uncertain_effect, checked_protected_file,
    uncertain_effects,
};
pub use crate::workspace_kind::ProductWorkspaceKind;
