//! Daemon-side product coding-run execution.
//! This crate coordinates concrete provider calls, managed-worktree edits, repository gates, and
//! an independent review/fix cycle. It owns no UI and grants no authority; the daemon supplies
//! already-resolved provider and workspace capabilities.
mod accounting;
pub mod attachment;
pub use peritus_tools_computer::{
    ensure_setup as computer_ensure_setup, setup as computer_setup, status as computer_status,
};
mod attachment_read;

mod budget;

pub(crate) mod bundle;

mod candidate;
mod context_config;
pub mod context_projection;
pub mod control;
mod conversation_mode;

mod delivery_requirement;

mod design;
pub(crate) mod developer_tools;

mod diagnostic;
mod discard_recovery;

mod efficiency;
mod engineering_workflow;
mod error;

mod execution;

pub(crate) mod failover;

mod file_metadata;
mod file_observation;

pub(crate) mod gates;

mod guided_design;

mod local_context;

mod model_output;

mod progress;

pub mod qualification;

mod review;

mod reviewer_turn;

pub(crate) mod trace;

mod turn;

pub mod tool_output;

mod workspace_delivery;
pub use workspace_delivery::scope::ScopedBaseline;

mod workspace_filter;
mod workspace_kind;

mod workspace_media;

pub use workspace_media::{WorkspaceImageDiscovery, WorkspaceImagePage};

mod api;

pub use api::*;

pub use trace::{DeveloperTraceFrameKind, TraceFrame, TraceReadError, read_frames};
