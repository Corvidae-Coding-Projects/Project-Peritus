//! Structured Git tools for immutable and authorized Peritus workspaces.

mod catalog;
mod decoder;
mod dispatch_support;
mod dispatcher;
mod error;
mod input;
mod read;
mod render;
mod schemas;
mod verified;

pub use catalog::{descriptor_catalog, descriptor_digest};
pub use dispatcher::{GitDispatchKind, GitDispatcher, GitMutationOutcome};
pub use error::{GitToolError, GitToolErrorKind, GitToolOperation, RecoveryClass};
pub use input::{
    CandidateInput, DiffInput, HistoryInput, RollbackInput, SnapshotInput, StatusInput,
};
pub use read::{GitReadService, RetainedSnapshotObservation, SnapshotObservation};
pub use render::RenderedOutput;
