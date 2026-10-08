//! Structured Git tools for immutable and authorized Peritus workspaces.

mod catalog;
mod cursor;
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
    StatusPageInput,
};
pub use read::{
    DiffObservationPage, GitReadService, HistoryObservationPage, RetainedSnapshotObservation,
    SnapshotObservation, StatusObservationPage,
};
pub use render::RenderedOutput;
