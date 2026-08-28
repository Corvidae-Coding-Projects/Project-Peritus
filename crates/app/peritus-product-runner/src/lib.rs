//! Daemon-side product coding-run execution.
//!
//! This crate coordinates concrete provider calls, managed-worktree edits, repository gates, and
//! an independent review/fix cycle. It owns no UI and grants no authority; the daemon supplies
//! already-resolved provider and workspace capabilities.

pub(crate) mod bundle;
mod error;
mod execution;
pub(crate) mod gates;
pub(crate) mod plan;
pub(crate) mod provider;

pub use error::{ProductRunnerError, ProductRunnerErrorKind};
pub use execution::{
    ProductRunInput, ProductRunOutput, ProductRunPhase, ProductRunUpdate, ProductRunner,
    RoleProviders, RunObserver,
};
