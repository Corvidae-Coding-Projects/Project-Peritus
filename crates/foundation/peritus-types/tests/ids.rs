//! Boundary tests shared by every nominal identifier type.

use peritus_types::{
    AcceptanceSpecId, ActionId, ActorId, ApprovalRequestId, ArtifactId, AttemptId, BudgetId,
    BudgetReservationId, CommandId, EnvironmentId, EvaluationCampaignId, EventId, EvidenceId,
    EvolutionCampaignId, FindingId, GateExecutionId, GateId, HarnessId, IdentifierError, PolicyId,
    ProcessId, ProjectId, ProviderProfileId, ResourceId, ReviewCycleId, RunId, SessionId,
    SnapshotId, TurnId, WorkspaceId,
};
use std::collections::BTreeSet;

fn assert_identifier<T: Copy + std::fmt::Debug + Eq>(
    new: fn([u8; 16]) -> Result<T, IdentifierError>,
    into_bytes: fn(T) -> [u8; 16],
) {
    assert_eq!(new([0; 16]), Err(IdentifierError::Zero));

    let mut bytes = [0; 16];
    bytes[15] = 1;
    let identifier = new(bytes).expect("a nonzero identifier must be accepted");
    assert_eq!(into_bytes(identifier), bytes);
}

#[test]
fn every_nominal_identifier_enforces_the_shared_boundary() {
    assert_identifier(ProjectId::new, ProjectId::into_bytes);
    assert_identifier(AcceptanceSpecId::new, AcceptanceSpecId::into_bytes);
    assert_identifier(HarnessId::new, HarnessId::into_bytes);
    assert_identifier(SessionId::new, SessionId::into_bytes);
    assert_identifier(RunId::new, RunId::into_bytes);
    assert_identifier(AttemptId::new, AttemptId::into_bytes);
    assert_identifier(TurnId::new, TurnId::into_bytes);
    assert_identifier(ActionId::new, ActionId::into_bytes);
    assert_identifier(WorkspaceId::new, WorkspaceId::into_bytes);
    assert_identifier(SnapshotId::new, SnapshotId::into_bytes);
    assert_identifier(ActorId::new, ActorId::into_bytes);
    assert_identifier(EnvironmentId::new, EnvironmentId::into_bytes);
    assert_identifier(ResourceId::new, ResourceId::into_bytes);
    assert_identifier(PolicyId::new, PolicyId::into_bytes);
    assert_identifier(ProviderProfileId::new, ProviderProfileId::into_bytes);
    assert_identifier(CommandId::new, CommandId::into_bytes);
    assert_identifier(EventId::new, EventId::into_bytes);
    assert_identifier(ProcessId::new, ProcessId::into_bytes);
    assert_identifier(ArtifactId::new, ArtifactId::into_bytes);
    assert_identifier(EvidenceId::new, EvidenceId::into_bytes);
    assert_identifier(GateId::new, GateId::into_bytes);
    assert_identifier(GateExecutionId::new, GateExecutionId::into_bytes);
    assert_identifier(ReviewCycleId::new, ReviewCycleId::into_bytes);
    assert_identifier(FindingId::new, FindingId::into_bytes);
    assert_identifier(ApprovalRequestId::new, ApprovalRequestId::into_bytes);
    assert_identifier(BudgetId::new, BudgetId::into_bytes);
    assert_identifier(BudgetReservationId::new, BudgetReservationId::into_bytes);
    assert_identifier(EvaluationCampaignId::new, EvaluationCampaignId::into_bytes);
    assert_identifier(EvolutionCampaignId::new, EvolutionCampaignId::into_bytes);
}

#[test]
fn every_byte_position_can_make_an_identifier_nonzero() {
    for index in 0..ProjectId::LENGTH {
        let mut bytes = [0; 16];
        bytes[index] = 1;
        let identifier = ProjectId::new(bytes).expect("one nonzero byte must be sufficient");
        assert_eq!(identifier.as_bytes(), &bytes);
    }
}

#[test]
fn identifiers_have_value_semantics_and_nominal_types() {
    let low = ProjectId::new([1; 16]).expect("nonzero");
    let high = ProjectId::new([2; 16]).expect("nonzero");
    let mut values = BTreeSet::new();
    assert!(values.insert(high));
    assert!(values.insert(low));
    assert!(!values.insert(low));
    assert_eq!(values.into_iter().collect::<Vec<_>>(), vec![low, high]);

    let run = RunId::new([1; 16]).expect("nonzero");
    assert_eq!(run.into_bytes(), low.into_bytes());
}
