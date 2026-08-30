//! Daemon-side product coding-run execution.
//!
//! This crate coordinates concrete provider calls, managed-worktree edits, repository gates, and
//! an independent review/fix cycle. It owns no UI and grants no authority; the daemon supplies
//! already-resolved provider and workspace capabilities.

#[cfg(not(verus_only))]
mod budget;
#[cfg(not(verus_only))]
pub(crate) mod bundle;
#[cfg(not(verus_only))]
mod candidate;
#[cfg(not(verus_only))]
mod delivery_requirement;
#[cfg(not(verus_only))]
mod design;
#[cfg(not(verus_only))]
pub(crate) mod developer_tools;
#[cfg(not(verus_only))]
mod engineering_workflow;
mod error;
#[cfg(not(verus_only))]
mod execution;
#[cfg(not(verus_only))]
pub(crate) mod failover;
#[cfg(not(verus_only))]
mod file_metadata;
#[cfg(not(verus_only))]
pub(crate) mod gates;
#[cfg(not(verus_only))]
mod progress;
#[cfg(not(verus_only))]
mod review;
#[cfg(not(verus_only))]
mod reviewer_turn;
#[cfg(not(verus_only))]
pub(crate) mod trace;
#[cfg(not(verus_only))]
mod turn;
#[cfg(verus_only)]
mod verified_api;
#[cfg(not(verus_only))]
mod workspace_filter;
#[cfg(not(verus_only))]
mod workspace_media;

#[cfg(not(verus_only))]
pub use budget::{
    PRODUCT_RUN_MAX_COST_MICROUNITS, PRODUCT_RUN_MAX_ELAPSED, PRODUCT_RUN_MAX_MODEL_REQUESTS,
    PRODUCT_RUN_MAX_PEAK_RSS_BYTES, PRODUCT_RUN_MAX_TOOL_CALLS, PRODUCT_RUN_MAX_TOTAL_TOKENS,
    PRODUCT_RUN_MAX_WORKSPACE_GROWTH_BYTES, ProductRunProgress,
};
pub use error::{ProductRunnerError, ProductRunnerErrorKind};
#[cfg(not(verus_only))]
pub use execution::{
    ConversationView, ProductDeliveryScope, ProductRunInput, ProductRunOutcome, ProductRunOutput,
    ProductRunPhase, ProductRunUpdate, ProductRunner, RoleProviders, RunObserver,
};
#[cfg(verus_only)]
pub use verified_api::{
    ConversationView, PRODUCT_RUN_MAX_COST_MICROUNITS, PRODUCT_RUN_MAX_ELAPSED,
    PRODUCT_RUN_MAX_MODEL_REQUESTS, PRODUCT_RUN_MAX_PEAK_RSS_BYTES, PRODUCT_RUN_MAX_TOOL_CALLS,
    PRODUCT_RUN_MAX_TOTAL_TOKENS, PRODUCT_RUN_MAX_WORKSPACE_GROWTH_BYTES, ProductDeliveryScope,
    ProductRunInput, ProductRunOutcome, ProductRunOutput, ProductRunPhase, ProductRunProgress,
    ProductRunUpdate, ProductRunner, RoleProviders, RunObserver,
};
