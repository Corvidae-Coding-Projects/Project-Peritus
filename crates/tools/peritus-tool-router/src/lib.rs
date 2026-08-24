//! Exact-authority C4 tool router and sole invocation-permit owner.

mod authorization;
mod dispatch;
mod dispatcher;
mod error;
mod execution;
mod exposure;
mod intent;
mod normalization;
mod preparation;
mod recovery;
mod registry;
mod replay;
mod router;
mod verified;

pub use authorization::ToolAuthorizationRequest;
pub use dispatch::{
    AuthorizedInvocation, AuthorizedToolBinding, DispatchOutcome, InvocationHandle,
};
pub use dispatcher::{
    DispatchFailure, ExecutionUpdate, RecoveryObservation, ToolDispatcher, ToolExecution, ToolStart,
};
pub use error::{RouterError, RouterErrorKind};
pub use exposure::ExposedTools;
pub use intent::{TOOL_INTENT_MEDIA_TYPE, ToolIntentPayload, tool_action_intent};
pub use recovery::{RecoveryClassification, RecoveryOutcome, ReplayDisposition};
pub use registry::ToolRegistry;
pub use router::{RouterLimits, ToolRouter};
pub use verified::{
    ToolAuthorityFacts, tool_authority_complete, tool_exposure_complete,
    tool_lifecycle_transition_valid, tool_operation_refinement_complete,
    tool_rejection_effect_count_valid, tool_replay_transition_valid,
};
