//! Nominal identifier types grouped by domain responsibility.

mod base;
mod evolution;
mod execution;
mod lifecycle;
mod records;
mod review;

#[cfg(verus_only)]
pub use base::valid_identifier_bytes;
pub use evolution::{EvaluationCampaignId, EvolutionCampaignId};
pub use execution::{
    ActionId, ActorId, CommandId, EnvironmentId, PolicyId, ProviderProfileId, ResourceId,
    SnapshotId, TurnId, WorkspaceId,
};
pub use lifecycle::{AcceptanceSpecId, AttemptId, HarnessId, ProjectId, RunId, SessionId};
pub use records::{ArtifactId, EventId, EvidenceId, ProcessId};
pub use review::{ApprovalRequestId, FindingId, GateExecutionId, GateId, ReviewCycleId};
