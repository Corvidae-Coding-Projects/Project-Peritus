//! Explicit public facade; implementation modules retain their existing ownership.

#[cfg(not(verus_only))]
pub use crate::budget::{
    PRODUCT_RUN_MAX_COST_MICROUNITS, PRODUCT_RUN_MAX_ELAPSED, PRODUCT_RUN_MAX_MODEL_REQUESTS,
    PRODUCT_RUN_MAX_PEAK_RSS_BYTES, PRODUCT_RUN_MAX_TOOL_CALLS, PRODUCT_RUN_MAX_TOTAL_TOKENS,
    PRODUCT_RUN_MAX_WORKSPACE_GROWTH_BYTES, ProductRunProgress,
};
pub use crate::context_config::{
    LocalCompactorSandbox, LocalContextConfig, LocalContextEngine, LocalProcessConfig,
    LocalSemanticBackend,
};
pub use crate::conversation_mode::ConversationMode;
#[cfg(not(verus_only))]
pub use crate::developer_tools::{
    CommandRuntime, FolderPatchAuthority, FolderPatchAuthorityPlan, checked_protected_file,
};
pub use crate::developer_tools::{
    FolderPatchAuthorityPlanRequest, PreviewCommand, PreviewLaunch, PreviewObservation,
    PreviewProcessState,
};
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
    PRODUCT_RUN_MAX_COST_MICROUNITS, PRODUCT_RUN_MAX_ELAPSED, PRODUCT_RUN_MAX_MODEL_REQUESTS,
    PRODUCT_RUN_MAX_PEAK_RSS_BYTES, PRODUCT_RUN_MAX_TOOL_CALLS, PRODUCT_RUN_MAX_TOTAL_TOKENS,
    PRODUCT_RUN_MAX_WORKSPACE_GROWTH_BYTES, ProductDeliveryScope, ProductRunInput,
    ProductRunOutcome, ProductRunOutput, ProductRunPhase, ProductRunProgress, ProductRunQuestion,
    ProductRunResume, ProductRunUpdate, ProductRunner, RoleProviders, RunObserver,
    WorkspaceMutationKind, checked_protected_file,
};
pub use crate::workspace_kind::ProductWorkspaceKind;
