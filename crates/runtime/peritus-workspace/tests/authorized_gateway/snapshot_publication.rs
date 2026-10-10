//! Snapshot publication recovery through the authorized workspace gateway.

use super::*;

#[test]
fn candidate_quota_failure_recovers_from_retained_snapshot_without_repeating_effects() {
    let temp = TempDir::new().expect("temporary root");
    let ids = Ids::new();
    let fixture = workspace_fixture(&temp, &ids, "candidate-quota");
    let persistence = fixture.persistence.clone();
    let prior_snapshot_manifest = fixture.initial.manifest().bytes().to_vec();
    let mut gateway = fixture.gateway;
    let mutation = authorized_patch(&temp, &ids, &mut gateway, fixture.patch);
    let candidate_ids = ids.for_action_revision(41, RevisionNumber::first());
    let successor = SnapshotId::new([86; 16]).expect("candidate snapshot");
    let candidate_intent =
        intent(&candidate_ids, candidate_authorization_payload(&mutation, successor));
    let candidate_receipts = receipts(&temp, &candidate_ids, &candidate_intent);
    let request = exact_request(&candidate_intent, &candidate_receipts, &candidate_ids);
    let action_digest = request.action_digest().expect("candidate action digest");
    let payload_digest = request.action_payload_digest();
    let undersized = artifact_store(&temp, "candidate-quota-artifacts", 1);

    let error = gateway
        .create_candidate(&request, &mutation, successor, &undersized)
        .err()
        .expect("manifest finalization must fail");

    assert_eq!(error.code(), peritus_workspace::ErrorCode::Artifact);
    assert_eq!(gateway.state().condition(), WorkspaceCondition::Dirty);
    assert_eq!(gateway.state().revision(), RevisionNumber::first());
    assert_snapshot_reference_present(&fixture.source, ids.workspace, successor);
    drop(undersized);
    drop(gateway);

    let artifacts = artifact_store(&temp, "candidate-quota-artifacts", 1_048_576);
    let prior_binding = ActionConsumptionBinding::new(
        ids.workspace,
        ids.resource,
        ids.environment,
        Generation::first(),
        RevisionNumber::first(),
    );
    let mut recovered_gateway = reopen_fixture_with_snapshot_condition(
        &persistence,
        &ids,
        &prior_snapshot_manifest,
        RevisionNumber::first(),
        WorkspaceCondition::Dirty,
    );
    let recovered = recovered_gateway
        .recover_git_mutation(
            prior_binding,
            candidate_ids.action,
            action_digest,
            payload_digest,
            &artifacts,
        )
        .expect("recover exact candidate from retained snapshot");
    let GitMutationRecoveryOutcome::Candidate(candidate) = recovered else {
        panic!("candidate result was not recovered");
    };
    assert_eq!(candidate.snapshot().manifest().snapshot_id(), successor);
    assert_eq!(recovered_gateway.state().revision(), RevisionNumber::new(2).unwrap());
}

pub fn assert_snapshot_reference_present(
    source: &peritus_test_support::TemporaryRepository,
    workspace: peritus_types::WorkspaceId,
    snapshot: SnapshotId,
) {
    let reference = peritus_git::expected_snapshot_ref(workspace, snapshot);
    assert!(
        source.git_success(["show-ref", "--verify", "--quiet", reference.as_str()]).is_ok(),
        "candidate snapshot should remain retained for action recovery"
    );
}
