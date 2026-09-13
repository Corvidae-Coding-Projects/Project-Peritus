//! Daemon-side product coding-run execution.
//! This crate coordinates concrete provider calls, managed-worktree edits, repository gates, and
//! an independent review/fix cycle. It owns no UI and grants no authority; the daemon supplies
//! already-resolved provider and workspace capabilities.
pub mod attachment;
#[cfg(not(verus_only))]
mod budget;
#[cfg(not(verus_only))]
pub(crate) mod bundle;
#[cfg(not(verus_only))]
mod candidate;
mod context_config;
pub mod control;
mod conversation_mode;
#[cfg(not(verus_only))]
mod delivery_requirement;
#[cfg(not(verus_only))]
mod design;
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
mod local_context;
#[cfg(not(verus_only))]
mod progress;
#[cfg(not(verus_only))]
pub mod qualification;
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
mod workspace_delivery;
#[cfg(not(verus_only))]
mod workspace_filter;
mod workspace_kind;
#[cfg(not(verus_only))]
mod workspace_media;

mod api;
mod api_parity;
pub use api::*;
