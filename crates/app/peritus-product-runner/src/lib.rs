//! Daemon-side product coding-run execution.
//! This crate coordinates concrete provider calls, managed-worktree edits, repository gates, and
//! an independent review/fix cycle. It owns no UI and grants no authority; the daemon supplies
//! already-resolved provider and workspace capabilities.
mod accounting;
pub mod attachment;
mod attachment_read;

mod budget;

pub(crate) mod bundle;

mod candidate;
mod context_config;
pub mod control;
mod conversation_mode;

mod delivery_requirement;

mod design;
pub(crate) mod developer_tools;

mod diagnostic;
mod discard_recovery;

mod engineering_workflow;
mod error;

mod execution;

pub(crate) mod failover;

mod file_metadata;

pub(crate) mod gates;

mod local_context;

mod model_output;

mod progress;

pub mod qualification;

mod review;

mod reviewer_turn;

pub(crate) mod trace;

mod turn;

mod workspace_delivery;

mod workspace_filter;
mod workspace_kind;

mod workspace_media;

pub use workspace_media::{WorkspaceImageDiscovery, WorkspaceImagePage};

mod api;

pub use api::*;

pub use trace::DeveloperTraceFrameKind;
