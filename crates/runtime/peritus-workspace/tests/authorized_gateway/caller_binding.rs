//! Parent tool-to-child workspace authority binding regressions.

use peritus_patch::PatchSet;
use peritus_policy::ActorRole;
use peritus_types::{ActionId, CapabilityName, Sha256Digest, SnapshotId};
use peritus_workspace::{
    MutationOutcome, RollbackRequest, WorkspaceCallerBinding, WorkspaceState,
    candidate_authorization_payload, candidate_authorization_payload_for_caller,
    patch_authorization_payload, patch_authorization_payload_for_caller,
    predicted_candidate_authorization_payload, rollback_authorization_payload,
    rollback_authorization_payload_for_caller,
};
use tempfile::TempDir;

use super::{
    assert_no_effect,
    authority_support::{Ids, intent, mismatched_preimage_patch, receipts, workspace_fixture},
    exact_request,
};

#[test]
fn distinct_tool_parent_binding_is_committed_and_swaps_have_no_effect() {
    let temp = TempDir::new().expect("temporary root");
    let ids = Ids::new();
    let exact_binding = WorkspaceCallerBinding::new(
        ActionId::new([71; 16]).expect("distinct parent tool action"),
        ids.actor,
        ActorRole::Writer,
        ids.workspace,
        ids.environment,
        ids.resource,
        CapabilityName::new("fs.write".to_owned()).expect("parent tool capability"),
        Sha256Digest::new([72; 32]),
        Sha256Digest::new([73; 32]),
    );

    reject_swapped_prepared_digest(&temp, &ids, &exact_binding);
    accept_distinct_parent_action(&temp, &ids, &exact_binding);
    assert_legacy_payload_unchanged(&ids, &exact_binding);
}

fn reject_swapped_prepared_digest(
    temp: &TempDir,
    ids: &Ids,
    exact_binding: &WorkspaceCallerBinding,
) {
    let rejected = workspace_fixture(temp, ids, "caller-swap-rejected");
    let mut gateway = rejected.gateway;
    let payload = patch_authorization_payload_for_caller(&rejected.patch, exact_binding);
    assert!(payload.starts_with(b"PERITUS-WORKSPACE-PATCH-V2\0"));
    let action = intent(ids, payload);
    let committed = receipts(temp, ids, &action);
    let swapped = WorkspaceCallerBinding::new(
        exact_binding.action_id(),
        exact_binding.actor_id(),
        exact_binding.role(),
        exact_binding.workspace_id(),
        exact_binding.environment_id(),
        exact_binding.resource_id(),
        exact_binding.capability_name().clone(),
        exact_binding.descriptor_digest(),
        Sha256Digest::new([74; 32]),
    );
    let request = exact_request(&action, &committed, ids).with_caller_binding(swapped);
    assert!(gateway.apply_patch(&request, rejected.patch).is_err());
    assert_no_effect(&gateway);
}

fn accept_distinct_parent_action(
    temp: &TempDir,
    ids: &Ids,
    exact_binding: &WorkspaceCallerBinding,
) {
    let accepted = workspace_fixture(temp, ids, "caller-distinct-accepted");
    let mut gateway = accepted.gateway;
    let action =
        intent(ids, patch_authorization_payload_for_caller(&accepted.patch, exact_binding));
    let committed = receipts(temp, ids, &action);
    let request =
        exact_request(&action, &committed, ids).with_caller_binding(exact_binding.clone());
    let outcome = gateway
        .apply_patch(&request, accepted.patch)
        .expect("distinct parent tool action binds through child workspace authorization");
    assert_eq!(outcome.action_id(), ids.action);
    assert_ne!(outcome.action_id(), exact_binding.action_id());
}

fn assert_legacy_payload_unchanged(ids: &Ids, exact_binding: &WorkspaceCallerBinding) {
    let legacy_patch = mismatched_preimage_patch(ids);
    let legacy = patch_authorization_payload(&legacy_patch);
    let mut expected = b"PERITUS-WORKSPACE-PATCH-V1\0".to_vec();
    append_legacy_patch_fields(&mut expected, &legacy_patch);
    assert_eq!(legacy, expected);
    assert_eq!(
        patch_authorization_payload_for_caller(&mismatched_preimage_patch(ids), exact_binding),
        patch_authorization_payload_for_caller(&mismatched_preimage_patch(ids), exact_binding)
    );
}

fn append_legacy_patch_fields(bytes: &mut Vec<u8>, patch: &PatchSet) {
    bytes.extend_from_slice(patch.workspace_id().as_bytes());
    bytes.extend_from_slice(&patch.expected_generation().get().to_be_bytes());
    bytes.extend_from_slice(&patch.expected_revision().get().to_be_bytes());
    bytes.extend_from_slice(patch.identity().as_bytes());
}

pub fn assert_candidate_payload(
    mutation: &MutationOutcome,
    snapshot: SnapshotId,
    caller: &WorkspaceCallerBinding,
) {
    let mut expected = b"PERITUS-WORKSPACE-CANDIDATE-V1\0".to_vec();
    expected.extend_from_slice(mutation.action_id().as_bytes());
    expected.extend_from_slice(mutation.workspace_id().as_bytes());
    expected.extend_from_slice(mutation.resource_id().as_bytes());
    expected.extend_from_slice(&mutation.generation().get().to_be_bytes());
    expected.extend_from_slice(&mutation.revision().get().to_be_bytes());
    expected.extend_from_slice(mutation.patch_identity().as_bytes());
    expected.extend_from_slice(snapshot.as_bytes());
    assert_eq!(candidate_authorization_payload(mutation, snapshot), expected);
    assert_eq!(
        candidate_authorization_payload_for_caller(mutation, snapshot, caller),
        candidate_authorization_payload_for_caller(mutation, snapshot, caller)
    );
}

pub fn assert_predicted_candidate_payload(mutation: &MutationOutcome, snapshot: SnapshotId) {
    assert_eq!(
        candidate_authorization_payload(mutation, snapshot),
        predicted_candidate_authorization_payload(
            mutation.action_id(),
            mutation.workspace_id(),
            mutation.resource_id(),
            mutation.generation(),
            mutation.revision(),
            mutation.patch_identity(),
            snapshot,
        ),
    );
}

pub fn assert_rollback_payload(
    state: &WorkspaceState,
    request: &RollbackRequest<'_>,
    caller: &WorkspaceCallerBinding,
) {
    let mut expected = b"PERITUS-WORKSPACE-ROLLBACK-V1\0".to_vec();
    expected.extend_from_slice(state.binding().workspace_id().as_bytes());
    expected.extend_from_slice(&state.generation().get().to_be_bytes());
    expected.extend_from_slice(&state.revision().get().to_be_bytes());
    append_object(&mut expected, request.target().commit().object_id());
    append_object(&mut expected, request.target().tree().object_id());
    expected.extend_from_slice(request.successor_snapshot_id().as_bytes());
    assert_eq!(rollback_authorization_payload(state, request), expected);
    assert_eq!(
        rollback_authorization_payload_for_caller(state, request, caller),
        rollback_authorization_payload_for_caller(state, request, caller)
    );
}

fn append_object(bytes: &mut Vec<u8>, object: peritus_git::ObjectId) {
    bytes.push(match object.format() {
        peritus_git::ObjectFormat::Sha1 => 1,
        peritus_git::ObjectFormat::Sha256 => 2,
    });
    bytes.extend_from_slice(object.as_bytes());
}
