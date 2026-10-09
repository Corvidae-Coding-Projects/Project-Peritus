//! Snapshot publication compensation through the authorized workspace gateway.

use super::*;

#[test]
fn candidate_quota_failure_releases_the_unpublished_snapshot() {
    let temp = TempDir::new().expect("temporary root");
    let ids = Ids::new();
    let fixture = workspace_fixture(&temp, &ids, "candidate-quota");
    let mut gateway = fixture.gateway;
    let mutation = authorized_patch(&temp, &ids, &mut gateway, fixture.patch);
    let candidate_ids = ids.for_action_revision(41, RevisionNumber::first());
    let successor = SnapshotId::new([86; 16]).expect("candidate snapshot");
    let candidate_intent =
        intent(&candidate_ids, candidate_authorization_payload(&mutation, successor));
    let candidate_receipts = receipts(&temp, &candidate_ids, &candidate_intent);
    let request = exact_request(&candidate_intent, &candidate_receipts, &candidate_ids);
    let undersized = artifact_store(&temp, "candidate-quota-artifacts", 1);

    let error = gateway
        .create_candidate(&request, &mutation, successor, &undersized)
        .err()
        .expect("manifest finalization must fail");

    assert_eq!(error.code(), peritus_workspace::ErrorCode::Artifact);
    assert_eq!(gateway.state().condition(), WorkspaceCondition::Dirty);
    assert_eq!(gateway.state().revision(), RevisionNumber::first());
    assert_snapshot_reference_absent(&fixture.source, ids.workspace, successor);
}

pub fn assert_snapshot_reference_absent(
    source: &peritus_test_support::TemporaryRepository,
    workspace: peritus_types::WorkspaceId,
    snapshot: SnapshotId,
) {
    let reference = peritus_git::expected_snapshot_ref(workspace, snapshot);
    assert!(
        source.git_success(["show-ref", "--verify", "--quiet", reference.as_str()]).is_err(),
        "failed snapshot publication left a retained Git reference"
    );
}
