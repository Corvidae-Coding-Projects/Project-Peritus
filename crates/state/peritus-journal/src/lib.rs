//! Transactional exact-byte event persistence for Peritus.
//!
//! The crate separates deterministic append planning from its `SQLite` durability adapter. Event
//! frames are validated once and are thereafter stored and exported byte-for-byte; they are never
//! decoded and re-encoded by the journal. A successful append returns a move-only
//! [`CommittedBatch`] which can only be constructed from an exact post-commit observation.

mod append_plan;
mod application;
mod authority;
mod domain;
mod error;
pub(crate) mod hash_chain;
mod head;
mod idempotency;
mod identity;
mod integrity;
mod outbox;
mod receipt;
mod record;
mod sqlite;
mod verified;

pub use append_plan::{
    AppendPlan, AppendRequest, HeadExpectation, MAX_BATCH_EVENTS,
    bind_outbox_acknowledgements_digest,
};
pub use application::{
    ApplicationArtifact, ApplicationArtifactState, ApplicationCommandAdmission,
    ApplicationCommandRecord, ApplicationCommandSettlement, ApplicationCommandState,
    ApplicationPrincipal, ApplicationPrincipalKind, ApplicationPrincipalState,
    ApplicationRequestId, ApplicationSession, ApplicationSessionState, ApplicationWorkspace,
    ApplicationWorkspacePage, ApplicationWorkspaceState, MAX_APPLICATION_WORKSPACE_PAGE,
    MAX_APPLICATION_WORKSPACE_REGISTRATION_BYTES, NewApplicationArtifact, NewApplicationCommand,
    NewApplicationPrincipal, NewApplicationSession, NewApplicationWorkspace,
};
pub use authority::{
    AllocatedAuthorityEpoch, AuthorityEpoch, CredentialRegistryInstall, CurrentAuthorityEpoch,
    ExpectedAuthorityEpoch,
};
pub use domain::{
    ApprovalCommitRequest, ApprovalUseCommitRequest, ApprovalUseResolution,
    ApprovalUseResolutionRequest, BudgetCommitRequest, CapabilityCommitRequest,
    CommittedApprovalTransition, CommittedApprovalUse, CommittedBudgetTransition,
    CommittedCapabilityUse, CommittedKernelTransition, CommittedLeaseTransition,
    KernelCommitRequest, KernelInputReference, KernelReplayCapsule, KernelReplayDriver,
    KernelReplayFailure, LeaseCommitRequest, NonActivationObservation, RecoveredKernelAggregate,
};
pub use error::{JournalError, JournalErrorKind, RecoveryClass};
pub use head::AggregateHead;
pub use idempotency::{CommandDecision, decide_command};
pub use identity::{AggregateId, AggregateKey, AggregateKind, OutboxId, StoreId};
pub use integrity::{CommittedArtifactReference, IntegrityExport, IntegrityReport};
pub use outbox::{OutboxAcknowledgement, OutboxDraft, OutboxMessage, OutboxState};
pub use receipt::{CommittedBatch, CurrentCredentialRegistry};
pub use record::{
    ArtifactDependency, CommittedRecord, DurableStateRecord, EventDraft, ExactFrame,
    GlobalEventWindow, MAX_GLOBAL_WINDOW_RECORDS, StateInstall,
};
pub use sqlite::{CommandResolution, SqliteJournal, SqliteJournalOptions, SqliteSettings};
