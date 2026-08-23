//! Target-owned authorization and lifecycle boundary for isolated Peritus workspaces.
//!
//! [`WorkspaceGateway`] is the only public mutation surface. It validates exact committed B0/B1
//! observations before constructing a private, one-use permit and immediately consuming it. A
//! [`ReadOnlyWorkspace`] is a distinct type fixed to an immutable snapshot.

mod authorization;
mod candidate;
mod consumption;
mod error;
mod gateway;
mod identity;
mod manifest;
mod mutation;
mod open;
mod read_only;
mod reconcile;
mod refinement;
mod rollback;
mod state;
mod transaction_namespace;
mod verified;
mod writable;

pub use authorization::WorkspaceAuthorizationRequest;
pub use candidate::{CandidateOutcome, candidate_authorization_payload};
pub use error::{ErrorCode, RecoveryClass, WorkspaceError, WorkspaceOperation};
pub use gateway::WorkspaceGateway;
pub use identity::{SnapshotIdentity, WorkspaceBinding};
pub use manifest::{ManifestKind, WorkspaceManifest};
pub use mutation::{MutationOutcome, patch_authorization_payload};
pub use open::{ReadOnlyOpenRequest, WritableOpenRequest};
pub use read_only::ReadOnlyWorkspace;
pub use reconcile::{
    ReconciliationEvidence, ReconciliationInput, ReconciliationOutcome, RestartDisposition,
    RestartObservation, classify as classify_restart,
};
pub use rollback::{RollbackOutcome, RollbackRequest, rollback_authorization_payload};
pub use state::{WorkspaceCondition, WorkspaceState};
pub use writable::WritableWorkspace;
