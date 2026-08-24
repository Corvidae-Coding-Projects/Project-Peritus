//! Exact committed B0/B1/C0 receipts through the production workspace mutation boundary.

mod authority_support;
#[path = "authorized_gateway/caller_binding.rs"]
mod caller_binding;
#[path = "authority_support/namespace_safety.rs"]
mod namespace_safety;
#[path = "authorized_gateway/tool_binding.rs"]
mod tool_binding;

use peritus_leases::{LeaseHolder, LeaseScope, ReconciliationCorrelation};
use peritus_patch::PatchSet;
use peritus_policy::{ActorRole, OperationClass};
use peritus_protocol::ActionIntentDto;
use peritus_types::{
    ActionId, ActorId, CapabilityName, EnvironmentId, EventId, Generation, ResourceId,
    RevisionNumber, RevisionTuple, SessionId, SnapshotId,
};
use peritus_workspace::{
    RestartDisposition, RollbackRequest, WorkspaceAuthorizationRequest, WorkspaceCondition,
    WorkspaceGateway, candidate_authorization_payload, candidate_authorization_payload_for_caller,
    patch_authorization_payload, rollback_authorization_payload,
    rollback_authorization_payload_for_caller,
};
use tempfile::TempDir;

use authority_support::{
    Ids, artifact_store, authorized_patch, commit_authority, intent, mismatched_preimage_patch,
    open_journal, receipts, reopen_fixture, try_reopen_fixture, workspace_fixture,
};
use tool_binding::tool_binding;

#[test]
fn exact_committed_receipts_are_required_before_real_patch_effect() {
    let temp = TempDir::new().expect("temporary root");
    let ids = Ids::new();
    let fixture = workspace_fixture(&temp, &ids, "authorized");
    let mut gateway = fixture.gateway;
    let patch = fixture.patch;
    let intent = ActionIntentDto {
        action_id: ids.action,
        actor_id: ids.actor,
        role: ActorRole::Writer,
        environment_id: ids.environment,
        resource_id: ids.resource,
        capability_name: ids.capability.clone(),
        operation_class: OperationClass::WorkspaceMutation,
        media_type: "application/vnd.peritus.workspace-patch.v1".to_owned(),
        payload: patch_authorization_payload(&patch),
    };
    let mut journal = open_journal(&temp);
    let receipts = commit_authority(&mut journal, &ids, &intent);
    assert_intent_drifts_have_no_effect(&mut gateway, &patch, &intent, &receipts, &ids);
    assert_request_drifts_have_no_effect(&mut gateway, &patch, &intent, &receipts, &ids);

    let authorized = WorkspaceAuthorizationRequest::new(
        &intent,
        &receipts.kernel,
        &receipts.capability,
        &receipts.lease,
        &receipts.epoch,
        ids.revision,
        ids.session,
        Generation::first(),
        RevisionNumber::first(),
        receipts.observed_at,
    );
    let outcome = gateway.apply_patch(&authorized, patch).expect("authorized patch");
    assert_eq!(outcome.action_id(), ids.action);
    assert_eq!(
        std::fs::read(gateway.state().binding().root().join("authorized.txt"))
            .expect("installed authorized file"),
        b"authorized\n",
    );
    assert_eq!(gateway.state().condition(), WorkspaceCondition::Dirty);
    assert_target_owned_wrong_holder_is_fenced_and_finalized(&temp, &mut gateway, &ids);
}

#[test]
fn gateway_runs_candidate_rollback_and_clean_reconciliation_with_real_effects() {
    let temp = TempDir::new().expect("temporary root");
    let ids = Ids::new();
    let fixture = workspace_fixture(&temp, &ids, "lifecycle");
    let mut gateway = fixture.gateway;
    let artifacts = artifact_store(&temp, "lifecycle-artifacts", 1_048_576);

    let patch_intent = intent(&ids, patch_authorization_payload(&fixture.patch));
    let patch_receipts = receipts(&temp, &ids, &patch_intent);
    let patch_request = exact_request(&patch_intent, &patch_receipts, &ids);
    let mutation =
        gateway.apply_patch(&patch_request, fixture.patch).expect("authorized real patch");
    assert_eq!(mutation.workspace_id(), ids.workspace);
    assert_eq!(mutation.resource_id(), ids.resource);

    let candidate_ids = ids.for_action_revision(21, RevisionNumber::first());
    let successor = SnapshotId::new([81; 16]).expect("candidate snapshot");
    let candidate_binding = tool_binding(&candidate_ids, 61, "git.candidate", 62, 63);
    caller_binding::assert_candidate_payload(&mutation, successor, &candidate_binding);
    let candidate_intent = intent(
        &candidate_ids,
        candidate_authorization_payload_for_caller(&mutation, successor, &candidate_binding),
    );
    let candidate_receipts = receipts(&temp, &candidate_ids, &candidate_intent);
    let swapped_candidate = tool_binding(&candidate_ids, 61, "git.candidate", 62, 64);
    let swapped_request = exact_request(&candidate_intent, &candidate_receipts, &candidate_ids)
        .with_caller_binding(swapped_candidate);
    assert!(gateway.create_candidate(&swapped_request, &mutation, successor, &artifacts).is_err());
    assert_eq!(gateway.state().condition(), WorkspaceCondition::Dirty);
    assert_eq!(gateway.state().revision(), RevisionNumber::first());
    let candidate_request = exact_request(&candidate_intent, &candidate_receipts, &candidate_ids)
        .with_caller_binding(candidate_binding);
    let candidate = gateway
        .create_candidate(&candidate_request, &mutation, successor, &artifacts)
        .expect("authorized candidate");
    assert_eq!(gateway.state().condition(), WorkspaceCondition::Clean);
    assert_eq!(gateway.state().revision(), RevisionNumber::new(2).expect("revision two"));
    artifacts.verify(candidate.artifact_digest()).expect("candidate artifact");

    let rollback_ids = ids.for_action_revision(22, RevisionNumber::new(2).expect("revision two"));
    let rollback_request = RollbackRequest::new(
        &fixture.initial,
        SnapshotId::new([82; 16]).expect("rollback successor"),
    );
    let rollback_binding = tool_binding(&rollback_ids, 65, "git.rollback", 66, 67);
    caller_binding::assert_rollback_payload(gateway.state(), &rollback_request, &rollback_binding);
    let rollback_intent = intent(
        &rollback_ids,
        rollback_authorization_payload_for_caller(
            gateway.state(),
            &rollback_request,
            &rollback_binding,
        ),
    );
    let rollback_receipts = receipts(&temp, &rollback_ids, &rollback_intent);
    let swapped_rollback = tool_binding(&rollback_ids, 65, "git.rollback", 66, 68);
    let swapped_authorization = exact_request(&rollback_intent, &rollback_receipts, &rollback_ids)
        .with_caller_binding(swapped_rollback);
    assert!(
        gateway
            .rollback(
                &swapped_authorization,
                RollbackRequest::new(
                    &fixture.initial,
                    SnapshotId::new([82; 16]).expect("rollback successor"),
                ),
                &artifacts,
            )
            .is_err()
    );
    assert_eq!(gateway.state().condition(), WorkspaceCondition::Clean);
    assert!(gateway.state().binding().root().join("authorized.txt").exists());
    let authorization = exact_request(&rollback_intent, &rollback_receipts, &rollback_ids)
        .with_caller_binding(rollback_binding);
    let rollback = gateway
        .rollback(&authorization, rollback_request, &artifacts)
        .expect("authorized rollback");
    assert_eq!(rollback.restored_from(), fixture.initial.commit());
    assert_eq!(gateway.state().condition(), WorkspaceCondition::Clean);
    assert_eq!(gateway.state().revision(), RevisionNumber::new(3).expect("revision three"));
    assert!(!gateway.state().binding().root().join("authorized.txt").exists());

    let expected = ReconciliationCorrelation::new(
        LeaseScope::new(ids.workspace, ids.resource, ids.environment),
        Generation::first(),
        ids.holder(),
    );
    let reconciled = gateway
        .reconcile_restart(
            expected,
            &artifacts,
            EventId::new([83; 16]).expect("reconciliation event"),
        )
        .expect("clean reconciliation");
    assert_eq!(reconciled.observation().disposition(), RestartDisposition::Clean);
    artifacts.verify(reconciled.artifact_digest()).expect("reconciliation artifact");
}

#[test]
fn rollback_stays_dirty_when_manifest_finalization_fails_after_restore() {
    let temp = TempDir::new().expect("temporary root");
    let ids = Ids::new();
    let fixture = workspace_fixture(&temp, &ids, "rollback-dirty");
    let mut gateway = fixture.gateway;
    let artifacts = artifact_store(&temp, "candidate-artifacts", 1_048_576);
    let mutation = authorized_patch(&temp, &ids, &mut gateway, fixture.patch);
    let candidate_ids = ids.for_action_revision(31, RevisionNumber::first());
    let successor = SnapshotId::new([84; 16]).expect("candidate snapshot");
    let candidate_intent =
        intent(&candidate_ids, candidate_authorization_payload(&mutation, successor));
    let candidate_receipts = receipts(&temp, &candidate_ids, &candidate_intent);
    let candidate_request = exact_request(&candidate_intent, &candidate_receipts, &candidate_ids);
    gateway
        .create_candidate(&candidate_request, &mutation, successor, &artifacts)
        .expect("authorized candidate");

    let rollback_ids = ids.for_action_revision(32, RevisionNumber::new(2).expect("revision two"));
    let rollback_request = RollbackRequest::new(
        &fixture.initial,
        SnapshotId::new([85; 16]).expect("rollback successor"),
    );
    let rollback_intent =
        intent(&rollback_ids, rollback_authorization_payload(gateway.state(), &rollback_request));
    let rollback_receipts = receipts(&temp, &rollback_ids, &rollback_intent);
    let authorization = exact_request(&rollback_intent, &rollback_receipts, &rollback_ids);
    let undersized = artifact_store(&temp, "undersized-artifacts", 1);
    let error = gateway
        .rollback(&authorization, rollback_request, &undersized)
        .err()
        .expect("manifest finalization must fail");
    assert_eq!(error.code(), peritus_workspace::ErrorCode::Artifact);
    assert_eq!(gateway.state().condition(), WorkspaceCondition::Dirty);
    assert_eq!(gateway.state().revision(), RevisionNumber::new(2).expect("unchanged revision"));
    assert!(!gateway.state().binding().root().join("authorized.txt").exists());
}

#[test]
fn durable_action_marker_rejects_receipt_replay_after_full_reopen() {
    let temp = TempDir::new().expect("temporary root");
    let ids = Ids::new();
    let fixture = workspace_fixture(&temp, &ids, "replay");
    let persistence = fixture.persistence.clone();
    let mut gateway = fixture.gateway;
    let failed_patch = mismatched_preimage_patch(&ids);
    let action = intent(&ids, patch_authorization_payload(&failed_patch));
    let committed = receipts(&temp, &ids, &action);
    let request = exact_request(&action, &committed, &ids);
    let first =
        gateway.apply_patch(&request, failed_patch.clone()).err().expect("preimage mismatch");
    assert_eq!(first.code(), peritus_workspace::ErrorCode::Patch);
    assert_eq!(gateway.state().condition(), WorkspaceCondition::Clean);
    drop(gateway.into_workspace());

    let mut reopened = reopen_fixture(&persistence, &ids);
    assert!(reopened.state().action_consumed(ids.action));
    let replay = reopened.apply_patch(&request, failed_patch).err().expect("receipt replay");
    assert_eq!(replay.code(), peritus_workspace::ErrorCode::ReceiptReused);
    assert_eq!(
        std::fs::read(reopened.state().binding().root().join("README.md")).expect("baseline file"),
        b"baseline\n"
    );
}

#[test]
fn clean_open_rejects_a_worktree_that_differs_from_the_current_snapshot_tree() {
    let temp = TempDir::new().expect("temporary root");
    let ids = Ids::new();
    let fixture = workspace_fixture(&temp, &ids, "dirty-open");
    let persistence = fixture.persistence.clone();
    let root = fixture.gateway.state().binding().root().to_owned();
    drop(fixture.gateway.into_workspace());
    std::fs::write(root.join("README.md"), b"drifted\n").expect("dirty tracked file");
    let error =
        try_reopen_fixture(&persistence, &ids).err().expect("clean reopen must reject drift");
    assert_eq!(error.operation(), peritus_workspace::WorkspaceOperation::Open);
}

fn assert_target_owned_wrong_holder_is_fenced_and_finalized(
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

fn assert_intent_drifts_have_no_effect(
    gateway: &mut WorkspaceGateway,
    patch: &PatchSet,
    exact: &ActionIntentDto,
    receipts: &authority_support::AuthorityReceipts,
    ids: &Ids,
) {
    let mut wrong_action = exact.clone();
    wrong_action.action_id = ActionId::new([91; 16]).expect("wrong action");
    let mut wrong_actor = exact.clone();
    wrong_actor.actor_id = ActorId::new([92; 16]).expect("wrong actor");
    let mut wrong_role = exact.clone();
    wrong_role.role = ActorRole::Reviewer;
    let mut wrong_environment = exact.clone();
    wrong_environment.environment_id = EnvironmentId::new([93; 16]).expect("wrong environment");
    let mut wrong_resource = exact.clone();
    wrong_resource.resource_id = ResourceId::new([94; 16]).expect("wrong resource");
    let mut wrong_capability = exact.clone();
    wrong_capability.capability_name =
        CapabilityName::new("workspace.other".to_owned()).expect("wrong capability");
    let mut wrong_payload = exact.clone();
    wrong_payload.payload.push(0xff);
    for drifted in [
        wrong_action,
        wrong_actor,
        wrong_role,
        wrong_environment,
        wrong_resource,
        wrong_capability,
        wrong_payload,
    ] {
        let request = exact_request(&drifted, receipts, ids);
        assert!(gateway.apply_patch(&request, patch.clone()).is_err());
        assert_no_effect(gateway);
    }
}

fn assert_request_drifts_have_no_effect(
    gateway: &mut WorkspaceGateway,
    patch: &PatchSet,
    intent: &ActionIntentDto,
    receipts: &authority_support::AuthorityReceipts,
    ids: &Ids,
) {
    let revision = RevisionTuple::new(
        ids.revision.acceptance_spec_id(),
        ids.revision.harness_id(),
        ids.revision.workspace_id(),
        ids.revision.workspace_generation(),
        RevisionNumber::new(2).expect("wrong request revision"),
        ids.revision.policy_id(),
        ids.revision.provider_profile_id(),
    );
    let wrong_session = SessionId::new([95; 16]).expect("wrong session");
    let next_generation = Generation::new(2).expect("stale generation");
    let next_revision = RevisionNumber::new(2).expect("stale revision");
    let expired = peritus_policy::AuthorityInstant::new(Generation::first(), 100);
    for (request_revision, session, generation, workspace_revision, observed_at) in [
        (revision, ids.session, Generation::first(), RevisionNumber::first(), receipts.observed_at),
        (
            ids.revision,
            wrong_session,
            Generation::first(),
            RevisionNumber::first(),
            receipts.observed_at,
        ),
        (ids.revision, ids.session, next_generation, RevisionNumber::first(), receipts.observed_at),
        (ids.revision, ids.session, Generation::first(), next_revision, receipts.observed_at),
        (ids.revision, ids.session, Generation::first(), RevisionNumber::first(), expired),
    ] {
        let request = WorkspaceAuthorizationRequest::new(
            intent,
            &receipts.kernel,
            &receipts.capability,
            &receipts.lease,
            &receipts.epoch,
            request_revision,
            session,
            generation,
            workspace_revision,
            observed_at,
        );
        assert!(gateway.apply_patch(&request, patch.clone()).is_err());
        assert_no_effect(gateway);
    }
}

const fn exact_request<'a>(
    intent: &'a ActionIntentDto,
    receipts: &'a authority_support::AuthorityReceipts,
    ids: &Ids,
) -> WorkspaceAuthorizationRequest<'a> {
    WorkspaceAuthorizationRequest::new(
        intent,
        &receipts.kernel,
        &receipts.capability,
        &receipts.lease,
        &receipts.epoch,
        ids.revision,
        ids.session,
        ids.revision.workspace_generation(),
        ids.revision.workspace_revision(),
        receipts.observed_at,
    )
}

fn assert_no_effect(gateway: &WorkspaceGateway) {
    assert_eq!(gateway.state().condition(), WorkspaceCondition::Clean);
    assert!(!gateway.state().binding().root().join("authorized.txt").exists());
}
