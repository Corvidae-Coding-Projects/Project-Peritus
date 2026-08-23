//! Structured Git repository and detached-worktree operations for Peritus.
//!
//! This crate is a hybrid effect adapter. Deterministic validation and reconciliation decisions
//! are separated from the fixed-shape Git subprocess boundary. It deliberately exposes no raw
//! Git argv API and never grants workspace mutation authority.

mod baseline;
mod command;
mod error;
mod manifest;
mod name;
mod object_id;
mod reconcile;
mod repository;
mod snapshot;
mod status;
mod verified;
mod worktree;

pub use baseline::Baseline;
pub use error::{ErrorKind, GitError, Operation, RecoveryClass};
pub use manifest::{
    CandidateSnapshotManifest, CandidateTreeManifest, WorktreeRegistrationManifest,
};
pub use name::WorktreeName;
pub use object_id::{CommitId, ObjectFormat, ObjectId, TreeId};
pub use reconcile::{
    DirtyReason, ReconcileDisposition, ReconcileExpectation, ReconcileObservation,
};
pub use repository::{GitRepository, RepositoryIdentity, RepositoryOptions};
pub use snapshot::{
    CandidateRequest, CandidateSnapshot, CandidateTree, RestoreObservation, RestoreRequest,
    SnapshotRef, SnapshotRequest,
};
pub use status::{
    ChangeCode, EntryModes, StatusEntry, StatusKind, StatusObservation, SubmoduleState,
};
pub use worktree::{
    CreateWorktree, RegisteredWorktree, RemovalPolicy, WorktreeAccess, WorktreeObservation,
};
