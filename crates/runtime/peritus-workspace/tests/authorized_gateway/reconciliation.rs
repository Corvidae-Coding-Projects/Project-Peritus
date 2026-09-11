//! Shared exact-owner reconciliation assertions.

use super::*;

pub fn assert_target_owned_wrong_holder_is_fenced_and_finalized(
    temp: &TempDir,
    gateway: &mut WorkspaceGateway,
    ids: &Ids,
) {
    let wrong_holder = LeaseHolder::new(
        ActorId::new([96; 16]).expect("wrong prior actor"),
        SessionId::new([97; 16]).expect("wrong prior session"),
    );
    let expected = ReconciliationCorrelation::new(
        LeaseScope::new(ids.workspace, ids.resource, ids.environment),
        Generation::first(),
        wrong_holder,
    );
    let artifacts = artifact_store(temp, "reconciliation-artifacts", 1_048_576);
    let outcome = gateway
        .reconcile_restart(
            expected,
            &artifacts,
            EventId::new([98; 16]).expect("reconciliation event"),
        )
        .expect("finalized reconciliation");
    assert_eq!(outcome.observation().disposition(), RestartDisposition::Fenced);
    assert_eq!(outcome.observation().evidence().correlation().prior_holder(), ids.holder(),);
    assert_eq!(outcome.manifest().action_id(), None);
    artifacts.verify(outcome.artifact_digest()).expect("verified reconciliation artifact");
}
