//! Exact committed authority for registered ordinary-folder patch application.

use std::{fs, path::Path};

use peritus_patch::{
    FileMode, FinalFile, LineEndingPolicy, PatchOperation, PatchSet, Preimage, WorkspacePath,
};
use peritus_workspace::{
    ErrorCode, FolderIdentity, FolderMutationActionMarker, FolderMutationCondition,
    FolderMutationGateway, FolderMutationOpenRequest, FolderMutationRecoveryRequest,
    FolderMutationRecoveryState, WorkspaceOperation, recover_folder_mutation,
};
use tempfile::TempDir;

use super::{Ids, authority_support, exact_request};

#[test]
fn registered_folder_applies_exact_bytes_with_committed_authority_and_rejects_replay() {
    let temporary = TempDir::new().expect("temporary root");
    let ids = Ids::new();
    let folder = temporary.path().join("registered-folder");
    fs::create_dir(&folder).expect("folder");
    fs::write(folder.join("AGENTS.md"), b"existing instructions\n").expect("preimage");
    let transaction_root = temporary.path().join("transactions");
    let mut gateway = open_gateway(&folder, &transaction_root, &ids);
    let patch = replace_patch(&ids, b"existing instructions\n", b"exact approved bytes\n");
    let payload = gateway.authorization_payload(&patch).expect("authorization payload");
    let intent = folder_intent(&ids, payload);
    let committed = authority_support::receipts(&temporary, &ids, &intent);
    let request = exact_request(&intent, &committed, &ids);

    let outcome =
        gateway.apply_patch(&request, patch.clone()).expect("authorized registered-folder patch");

    assert_eq!(fs::read(folder.join("AGENTS.md")).expect("postimage"), b"exact approved bytes\n");
    assert_eq!(outcome.action_id(), ids.action);
    assert_eq!(outcome.workspace_id(), ids.workspace);
    assert_eq!(outcome.resource_id(), ids.resource);
    assert_eq!(outcome.generation(), ids.revision.workspace_generation());
    assert_eq!(outcome.revision(), ids.revision.workspace_revision());
    assert_eq!(outcome.patch_identity(), patch.identity());
    assert!(!outcome.applied_patch().cleanup_pending());
    assert_eq!(outcome.condition(), FolderMutationCondition::Ready);
    assert_eq!(gateway.condition(), FolderMutationCondition::Ready);
    drop(gateway);

    let mut reopened = open_gateway(&folder, &transaction_root, &ids);
    let replay = reopened
        .apply_patch(&request, patch)
        .err()
        .expect("durably consumed action must not replay");
    assert_eq!(replay.code(), ErrorCode::ReceiptReused);
    assert_eq!(replay.operation(), WorkspaceOperation::Authorize);
    assert_eq!(
        fs::read(folder.join("AGENTS.md")).expect("replayed bytes"),
        b"exact approved bytes\n"
    );
    assert!(!folder.join(".git").exists());
}

#[test]
fn intervening_edit_conflicts_without_overwrite() {
    let temporary = TempDir::new().expect("temporary root");
    let ids = Ids::new();
    let folder = temporary.path().join("registered-folder");
    fs::create_dir(&folder).expect("folder");
    fs::write(folder.join("AGENTS.md"), b"observed instructions\n").expect("preimage");
    let transaction_root = temporary.path().join("transactions");
    let mut gateway = open_gateway(&folder, &transaction_root, &ids);
    let patch = replace_patch(&ids, b"observed instructions\n", b"proposed instructions\n");
    let payload = gateway.authorization_payload(&patch).expect("authorization payload");
    let intent = folder_intent(&ids, payload);
    let committed = authority_support::receipts(&temporary, &ids, &intent);
    let request = exact_request(&intent, &committed, &ids);
    fs::write(folder.join("AGENTS.md"), b"independent user edit\n").expect("intervening edit");

    let error = gateway.apply_patch(&request, patch).err().expect("stale preimage must conflict");

    assert_eq!(error.code(), ErrorCode::Patch);
    assert_eq!(error.operation(), WorkspaceOperation::Mutate);
    assert_eq!(gateway.condition(), FolderMutationCondition::Ready);
    assert_eq!(
        fs::read(folder.join("AGENTS.md")).expect("preserved edit"),
        b"independent user edit\n"
    );
    assert!(!folder.join(".git").exists());
}

#[test]
fn committed_authority_must_match_the_folder_owners_full_revision_tuple() {
    let temporary = TempDir::new().expect("temporary root");
    let ids = Ids::new();
    let folder = temporary.path().join("registered-folder");
    fs::create_dir(&folder).expect("folder");
    fs::write(folder.join("AGENTS.md"), b"observed instructions\n").expect("preimage");
    let mut owner_ids = Ids::new();
    owner_ids.revision = peritus_types::RevisionTuple::new(
        ids.revision.acceptance_spec_id(),
        ids.revision.harness_id(),
        ids.workspace,
        ids.revision.workspace_generation(),
        ids.revision.workspace_revision(),
        peritus_types::PolicyId::new([99; 16]).expect("different current policy"),
        ids.revision.provider_profile_id(),
    );
    let mut gateway = open_gateway(&folder, &temporary.path().join("transactions"), &owner_ids);
    let patch = replace_patch(&ids, b"observed instructions\n", b"proposed instructions\n");
    let payload = gateway.authorization_payload(&patch).expect("payload");
    let intent = folder_intent(&ids, payload);
    let committed = authority_support::receipts(&temporary, &ids, &intent);
    let authorization = exact_request(&intent, &committed, &ids);
    let error = gateway.apply_patch(&authorization, patch).err().expect("wrong policy rejected");
    assert_eq!(error.code(), ErrorCode::AuthorizationMismatch);
    assert_eq!(fs::read(folder.join("AGENTS.md")).expect("unchanged"), b"observed instructions\n");
}

#[test]
fn recovery_requires_the_exact_consumed_canonical_action_before_classifying_cleaned_state() {
    let temporary = TempDir::new().expect("temporary root");
    let ids = Ids::new();
    let folder = temporary.path().join("registered-folder");
    fs::create_dir(&folder).expect("folder");
    fs::write(folder.join("AGENTS.md"), b"before\n").expect("preimage");
    let transaction_root = temporary.path().join("transactions");
    let patch = replace_patch(&ids, b"before\n", b"after\n");

    let no_attempt = recover_folder_mutation(FolderMutationRecoveryRequest::new(
        FolderIdentity::observe(&folder).expect("identity"),
        ids.resource,
        ids.environment,
        ids.actor,
        ids.action,
        &transaction_root,
        patch.clone(),
    ))
    .expect("inspect absent attempt");
    assert_eq!(no_attempt.state(), FolderMutationRecoveryState::NoAttempt);
    assert_eq!(no_attempt.marker(), FolderMutationActionMarker::Missing);
    assert_eq!(fs::read(folder.join("AGENTS.md")).expect("preimage"), b"before\n");

    let mut gateway = open_gateway(&folder, &transaction_root, &ids);
    let payload = gateway.authorization_payload(&patch).expect("authorization payload");
    let intent = folder_intent(&ids, payload);
    let committed = authority_support::receipts(&temporary, &ids, &intent);
    let request = exact_request(&intent, &committed, &ids);
    gateway.apply_patch(&request, patch.clone()).expect("applied patch");
    drop(gateway);

    let recovered = recover_folder_mutation(FolderMutationRecoveryRequest::new(
        FolderIdentity::observe(&folder).expect("identity"),
        ids.resource,
        ids.environment,
        ids.actor,
        ids.action,
        &transaction_root,
        patch.clone(),
    ))
    .expect("inspect cleaned transaction");
    assert_eq!(recovered.state(), FolderMutationRecoveryState::ConsumedWithoutTransaction);
    assert_eq!(recovered.marker(), FolderMutationActionMarker::Exact);
    assert_eq!(recovered.patch_identity(), patch.identity());
    assert_eq!(fs::read(folder.join("AGENTS.md")).expect("postimage"), b"after\n");

    let wrong_actor = recover_folder_mutation(FolderMutationRecoveryRequest::new(
        FolderIdentity::observe(&folder).expect("identity"),
        ids.resource,
        ids.environment,
        peritus_types::ActorId::new([0xee; 16]).expect("wrong actor"),
        ids.action,
        &transaction_root,
        patch,
    ))
    .expect("wrong authority is classified without mutation");
    assert_eq!(wrong_actor.state(), FolderMutationRecoveryState::Indeterminate);
    assert_eq!(wrong_actor.marker(), FolderMutationActionMarker::DigestMismatch);
    assert_eq!(fs::read(folder.join("AGENTS.md")).expect("preserved bytes"), b"after\n");
}

fn open_gateway(folder: &Path, transaction_root: &Path, ids: &Ids) -> FolderMutationGateway {
    let identity = FolderIdentity::observe(folder).expect("folder identity");
    FolderMutationGateway::open(FolderMutationOpenRequest::new(
        identity,
        ids.resource,
        ids.environment,
        ids.revision,
        ids.holder(),
        transaction_root,
    ))
    .expect("folder gateway")
}

fn folder_intent(ids: &Ids, payload: Vec<u8>) -> peritus_protocol::ActionIntentDto {
    peritus_workspace::folder_patch_action_intent(
        ids.actor,
        ids.action,
        ids.environment,
        ids.resource,
        payload,
    )
    .expect("canonical folder-patch intent")
}

fn replace_patch(ids: &Ids, before: &[u8], after: &[u8]) -> PatchSet {
    let operation = PatchOperation::replace(
        WorkspacePath::new("AGENTS.md").expect("workspace path"),
        Preimage::from_bytes(before, FileMode::Regular),
        FinalFile::new(after.to_vec(), FileMode::Regular, LineEndingPolicy::Preserve)
            .expect("final file"),
    )
    .expect("replacement");
    PatchSet::new(
        ids.workspace,
        ids.revision.workspace_generation(),
        ids.revision.workspace_revision(),
        vec![operation],
    )
    .expect("patch")
}
