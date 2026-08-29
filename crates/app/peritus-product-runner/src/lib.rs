//! Daemon-side product coding-run execution.
//!
//! This crate coordinates concrete provider calls, managed-worktree edits, repository gates, and
//! an independent review/fix cycle. It owns no UI and grants no authority; the daemon supplies
//! already-resolved provider and workspace capabilities.

#[cfg(not(verus_only))]
pub(crate) mod bundle;
#[cfg(not(verus_only))]
mod candidate;
#[cfg(not(verus_only))]
mod design;
#[cfg(not(verus_only))]
mod developer_tools;
#[cfg(not(verus_only))]
mod engineering_workflow;
mod error;
#[cfg(not(verus_only))]
mod execution;
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
mod trace;
#[cfg(not(verus_only))]
mod turn;
#[cfg(verus_only)]
mod verified_api;
#[cfg(not(verus_only))]
mod workspace_filter;
#[cfg(not(verus_only))]
mod workspace_media;

pub use error::{ProductRunnerError, ProductRunnerErrorKind};
#[cfg(not(verus_only))]
pub use execution::{
    ConversationView, ProductRunInput, ProductRunOutcome, ProductRunOutput, ProductRunPhase,
    ProductRunUpdate, ProductRunner, RoleProviders, RunObserver,
};
#[cfg(verus_only)]
pub use verified_api::{
    ConversationView, ProductRunInput, ProductRunOutcome, ProductRunOutput, ProductRunPhase,
    ProductRunUpdate, ProductRunner, RoleProviders, RunObserver,
};
