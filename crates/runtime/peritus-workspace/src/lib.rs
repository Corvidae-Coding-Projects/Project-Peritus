//! Target-owned authorization and lifecycle boundary for isolated Peritus workspaces.
//!
//! [`WorkspaceGateway`] is the only public mutation surface. It validates exact committed B0/B1
//! observations before constructing a private, one-use permit and immediately consuming it. A
//! [`ReadOnlyWorkspace`] is a distinct type fixed to an immutable snapshot.

mod authorization;
mod caller;
mod candidate;
mod consumption;
mod error;
mod filesystem;
mod gateway;
mod git_inspection;
mod identity;
mod inspection;
mod manifest;
mod mutation;
mod open;
mod read_only;
mod reconcile;
mod refinement;
mod registration;
mod rollback;
mod state;
mod transaction_namespace;
mod verified;
mod writable;

pub use authorization::WorkspaceAuthorizationRequest;
pub use caller::{ReadOnlyTargetBinding, WorkspaceCallerBinding};
pub use candidate::{
    CandidateOutcome, candidate_authorization_payload, candidate_authorization_payload_for_caller,
    predicted_candidate_authorization_payload,
};
pub use error::{ErrorCode, RecoveryClass, WorkspaceError, WorkspaceOperation};
pub use gateway::WorkspaceGateway;
pub use identity::{SnapshotIdentity, WorkspaceBinding};
pub use inspection::{
    DirectoryEntry, MAX_INSPECTION_FILE_BYTES, WorkspaceEntryKind, WorkspaceMetadata,
};
pub use manifest::{ManifestKind, WorkspaceManifest};
pub use mutation::{
    MutationOutcome, patch_authorization_payload, patch_authorization_payload_for_caller,
};
pub use open::{ReadOnlyOpenRequest, WritableOpenRequest};
pub use read_only::ReadOnlyWorkspace;
pub use reconcile::{
    ReconciliationEvidence, ReconciliationInput, ReconciliationOutcome, RestartDisposition,
    RestartObservation, classify as classify_restart,
};
pub use registration::{MAX_WORKSPACE_REGISTRATION_BYTES, WorkspaceRegistration};
pub use rollback::{
    RollbackOutcome, RollbackRequest, rollback_authorization_payload,
    rollback_authorization_payload_for_caller,
};
pub use state::{WorkspaceCondition, WorkspaceState};
pub use writable::WritableWorkspace;
