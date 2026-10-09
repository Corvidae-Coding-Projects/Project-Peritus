//! Exact Git receipt recovery across lost terminal frames and failed publication.

use super::*;

#[test]
fn candidate_receipt_reopens_exact_result_without_repeating_git_effects() {
    candidate_recovery(false);
}

#[test]
fn candidate_recovers_interrupted_reference_publication_from_exact_retained_manifest() {
    candidate_recovery(true);
}

fn candidate_recovery(interrupt_reference_publication: bool) {
    let temp = TempDir::new().expect("temporary root");
    let ids = Ids::new();
    let fixture = workspace_fixture(&temp, &ids, "candidate-receipt");
    let persistence = fixture.persistence.clone();
    let prior_snapshot_manifest = fixture.initial.manifest().bytes().to_vec();
    let mut gateway = fixture.gateway;
    let transaction_namespace = gateway.transaction_namespace().to_owned();
    let artifacts = artifact_store(&temp, "candidate-receipt-artifacts", 1_048_576);
    let mutation = authorized_patch(&temp, &ids, &mut gateway, fixture.patch);
    let candidate_ids = ids.for_action_revision(41, RevisionNumber::first());
    let successor = SnapshotId::new([91; 16]).expect("candidate snapshot");
    let action = intent(&candidate_ids, candidate_authorization_payload(&mutation, successor));
    let committed = receipts(&temp, &candidate_ids, &action);
    let request = exact_request(&action, &committed, &candidate_ids);
    let candidate = gateway
        .create_candidate(&request, &mutation, successor, &artifacts)
        .expect("create candidate");
    let expected_commit = candidate.snapshot().commit();
    let expected_patch = candidate.patch_id();
    let expected_artifact = candidate.artifact_digest();
    if interrupt_reference_publication {
        interrupt_active_ref_publication(&fixture.source, candidate.snapshot());
    }
    let action_digest = candidate.manifest().action_digest().expect("action digest");
    let payload_digest = request.action_payload_digest();
    let prior_binding = ActionConsumptionBinding::new(
        ids.workspace,
        ids.resource,
        ids.environment,
        Generation::first(),
        RevisionNumber::first(),
    );
    drop(candidate);
    drop(gateway);

    truncate_action_terminal(
        &transaction_namespace,
        Generation::first(),
        RevisionNumber::first(),
        candidate_ids.action,
    );

    let mut gateway = reopen_fixture_with_snapshot_condition(
        &persistence,
        &ids,
        &prior_snapshot_manifest,
        RevisionNumber::first(),
        WorkspaceCondition::Dirty,
    );
    let recover = |gateway: &mut WorkspaceGateway, payload_digest| {
        gateway.recover_git_mutation(
            prior_binding,
            candidate_ids.action,
            action_digest,
            payload_digest,
            &artifacts,
        )
    };
    assert!(
        recover(&mut gateway, peritus_types::Sha256Digest::new([0x55; 32])).is_err(),
        "mismatched payload must fail before recovery publication"
    );
    assert_eq!(gateway.state().revision(), RevisionNumber::first());
    if interrupt_reference_publication {
        assert_conflicting_reference_is_preserved(
            &fixture.source,
            &peritus_git::expected_snapshot_ref(ids.workspace, successor),
            fixture.initial.commit(),
            &mut gateway,
            |gateway| recover(gateway, payload_digest).is_err(),
        );
    }
    let unrelated = gateway.state().binding().root().join("unrelated.txt");
    std::fs::write(&unrelated, b"unrelated user work\n").expect("intervening work");
    assert!(
        recover(&mut gateway, payload_digest).is_err(),
        "planned recovery must not mark unrelated work clean"
    );
    assert_eq!(gateway.state().revision(), RevisionNumber::first());
    assert_eq!(std::fs::read(&unrelated).expect("preserved"), b"unrelated user work\n");
    std::fs::remove_file(&unrelated).expect("remove test interference");
    let recovered = recover(&mut gateway, payload_digest)
        .expect("recover candidate receipt after lost terminal frame");
    let GitMutationRecoveryOutcome::Candidate(recovered) = recovered else {
        panic!("candidate receipt was not recovered");
    };
    assert_eq!(recovered.snapshot().commit(), expected_commit);
    assert_eq!(recovered.patch_id(), expected_patch);
    assert_eq!(recovered.artifact_digest(), expected_artifact);
    assert_eq!(gateway.state().revision(), RevisionNumber::new(2).expect("candidate revision"));
    assert_candidate_terminal_replays_exactly(
        &mut gateway,
        |gateway| recover(gateway, payload_digest),
        expected_commit,
        expected_artifact,
    );
}

fn interrupt_active_ref_publication(
    source: &peritus_test_support::TemporaryRepository,
    snapshot: &peritus_git::CandidateSnapshot,
) {
    source
        .git_success([
            "update-ref",
            "-d",
            snapshot.reference().as_str(),
            &snapshot.commit().to_string(),
        ])
        .expect("simulate interruption after companion manifest but before active ref");
}

fn assert_candidate_terminal_replays_exactly(
    gateway: &mut WorkspaceGateway,
    recover: impl Fn(
        &mut WorkspaceGateway,
    ) -> Result<GitMutationRecoveryOutcome, peritus_workspace::WorkspaceError>,
    expected_commit: peritus_git::CommitId,
    expected_artifact: peritus_artifact_store::ArtifactDigest,
) {
    let changed = gateway.state().binding().root().join("authorized.txt");
    let original = std::fs::read(&changed).expect("current bytes");
    std::fs::write(&changed, b"intervening edit\n").expect("intervening edit");
    assert!(recover(gateway).is_err(), "terminal replay must reject worktree drift");
    assert_eq!(std::fs::read(&changed).expect("preserved"), b"intervening edit\n");
    std::fs::write(&changed, original).expect("restore fixture bytes");
    let repeated = recover(gateway).expect("repeat receipt read");
    let GitMutationRecoveryOutcome::Candidate(repeated) = repeated else {
        panic!("candidate receipt changed on repeat");
    };
    assert_eq!(repeated.snapshot().commit(), expected_commit);
    assert_eq!(repeated.artifact_digest(), expected_artifact);
}

fn assert_conflicting_reference_is_preserved(
    source: &peritus_test_support::TemporaryRepository,
    reference: &peritus_git::SnapshotRef,
    conflicting_commit: peritus_git::CommitId,
    gateway: &mut WorkspaceGateway,
    recovery_fails: impl FnOnce(&mut WorkspaceGateway) -> bool,
) {
    assert!(
        source.git_success(["show-ref", "--verify", reference.as_str()]).is_err(),
        "wrong payload must not publish the missing reference"
    );
    let conflicting_commit = conflicting_commit.to_string();
    source
        .git_success(["update-ref", reference.as_str(), &conflicting_commit])
        .expect("install conflicting reference");
    assert!(recovery_fails(gateway), "recovery must not overwrite a conflicting reference");
    source
        .git_success(["update-ref", "-d", reference.as_str(), &conflicting_commit])
        .expect("conflicting reference remained unchanged");
}

#[test]
fn rollback_receipt_reopens_exact_result_without_repeating_git_effects() {
    rollback_recovery(false);
}

#[test]
fn interrupted_rollback_to_nonbaseline_tree_finalizes_missing_snapshot_after_restart() {
    rollback_recovery(true);
}

fn rollback_recovery(interrupt_before_snapshot: bool) {
    let temp = TempDir::new().expect("temporary root");
    let ids = Ids::new();
    let fixture = workspace_fixture(&temp, &ids, "rollback-receipt");
    let persistence = fixture.persistence.clone();
    let mut gateway = fixture.gateway;
    let transaction_namespace = gateway.transaction_namespace().to_owned();
    let artifacts = artifact_store(&temp, "rollback-receipt-artifacts", 1_048_576);
    let mutation = authorized_patch(&temp, &ids, &mut gateway, fixture.patch);
    let candidate_ids = ids.for_action_revision(51, RevisionNumber::first());
    let candidate_snapshot_id = SnapshotId::new([101; 16]).expect("candidate snapshot");
    let candidate_intent =
        intent(&candidate_ids, candidate_authorization_payload(&mutation, candidate_snapshot_id));
    let candidate_receipts = receipts(&temp, &candidate_ids, &candidate_intent);
    let candidate_request = exact_request(&candidate_intent, &candidate_receipts, &candidate_ids);
    let candidate = gateway
        .create_candidate(&candidate_request, &mutation, candidate_snapshot_id, &artifacts)
        .expect("create candidate");
    let prior_snapshot_manifest = candidate.snapshot().manifest().bytes().to_vec();
    let rollback_target =
        if interrupt_before_snapshot { candidate.snapshot().clone() } else { fixture.initial };
    let rollback_ids = ids.for_action_revision(52, RevisionNumber::new(2).expect("revision two"));
    let successor = SnapshotId::new([102; 16]).expect("rollback successor");
    let rollback_request = RollbackRequest::new(&rollback_target, successor);
    let rollback_intent =
        intent(&rollback_ids, rollback_authorization_payload(gateway.state(), &rollback_request));
    let rollback_receipts = receipts(&temp, &rollback_ids, &rollback_intent);
    let authorization = exact_request(&rollback_intent, &rollback_receipts, &rollback_ids);
    let rollback = gateway
        .rollback(&authorization, rollback_request, &artifacts)
        .expect("restore initial snapshot");
    let expected_commit = rollback.snapshot().commit();
    let expected_restored_from = rollback.restored_from();
    let expected_artifact = rollback.artifact_digest();
    let action_digest = rollback.manifest().action_digest().expect("action digest");
    let payload_digest = authorization.action_payload_digest();
    let prior_binding = ActionConsumptionBinding::new(
        ids.workspace,
        ids.resource,
        ids.environment,
        Generation::first(),
        RevisionNumber::new(2).expect("prior revision"),
    );
    if interrupt_before_snapshot {
        let repository = peritus_git::GitRepository::open(peritus_git::RepositoryOptions::new(
            fixture.source.root(),
        ))
        .expect("repository");
        repository
            .release_snapshot(rollback.snapshot())
            .expect("simulate interruption before retaining successor");
    }
    drop(rollback);
    drop(candidate);
    drop(gateway);
    truncate_action_terminal(
        &transaction_namespace,
        Generation::first(),
        RevisionNumber::new(2).expect("prior revision"),
        rollback_ids.action,
    );

    let mut gateway = reopen_fixture_with_snapshot_condition(
        &persistence,
        &ids,
        &prior_snapshot_manifest,
        RevisionNumber::new(2).expect("prior revision"),
        WorkspaceCondition::Dirty,
    );
    let recovered = gateway
        .recover_git_mutation(
            prior_binding,
            rollback_ids.action,
            action_digest,
            payload_digest,
            &artifacts,
        )
        .expect("recover rollback receipt");
    let GitMutationRecoveryOutcome::Rollback(recovered) = recovered else {
        panic!("rollback receipt was not recovered");
    };
    assert_eq!(recovered.snapshot().commit(), expected_commit);
    assert_eq!(recovered.restored_from(), expected_restored_from);
    assert_eq!(recovered.artifact_digest(), expected_artifact);
    assert_eq!(gateway.state().revision(), RevisionNumber::new(3).expect("rollback revision"));
    let repeated = gateway
        .recover_git_mutation(
            prior_binding,
            rollback_ids.action,
            action_digest,
            payload_digest,
            &artifacts,
        )
        .expect("repeat receipt read");
    let GitMutationRecoveryOutcome::Rollback(repeated) = repeated else {
        panic!("rollback receipt changed on repeat");
    };
    assert_eq!(repeated.snapshot().commit(), expected_commit);
    assert_eq!(repeated.artifact_digest(), expected_artifact);
}

#[test]
fn rollback_manifest_failure_recovers_exact_result_after_restart() {
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
    let candidate = gateway
        .create_candidate(&candidate_request, &mutation, successor, &artifacts)
        .expect("authorized candidate");
    let persistence = fixture.persistence.clone();
    let candidate_snapshot_manifest = candidate.snapshot().manifest().bytes().to_vec();
    drop(candidate);

    let rollback_ids = ids.for_action_revision(32, RevisionNumber::new(2).expect("revision two"));
    let rollback_snapshot = SnapshotId::new([85; 16]).expect("rollback successor");
    let rollback_request = RollbackRequest::new(&fixture.initial, rollback_snapshot);
    let rollback_intent =
        intent(&rollback_ids, rollback_authorization_payload(gateway.state(), &rollback_request));
    let rollback_receipts = receipts(&temp, &rollback_ids, &rollback_intent);
    let authorization = exact_request(&rollback_intent, &rollback_receipts, &rollback_ids);
    let action_digest = authorization.action_digest().expect("rollback action digest");
    let payload_digest = authorization.action_payload_digest();
    let undersized = artifact_store(&temp, "undersized-artifacts", 1);
    let error = gateway
        .rollback(&authorization, rollback_request, &undersized)
        .err()
        .expect("manifest finalization must fail");
    assert_eq!(error.code(), peritus_workspace::ErrorCode::Artifact);
    assert_eq!(gateway.state().condition(), WorkspaceCondition::Dirty);
    assert_eq!(gateway.state().revision(), RevisionNumber::new(2).expect("unchanged revision"));
    assert!(!gateway.state().binding().root().join("authorized.txt").exists());
    snapshot_publication::assert_snapshot_reference_present(
        &fixture.source,
        ids.workspace,
        rollback_snapshot,
    );
    drop(undersized);
    drop(gateway);
    let artifacts = artifact_store(&temp, "undersized-artifacts", 1_048_576);
    let mut gateway = reopen_fixture_with_snapshot_condition(
        &persistence,
        &ids,
        &candidate_snapshot_manifest,
        RevisionNumber::new(2).expect("revision two"),
        WorkspaceCondition::Dirty,
    );
    let prior_binding = ActionConsumptionBinding::new(
        ids.workspace,
        ids.resource,
        ids.environment,
        Generation::first(),
        RevisionNumber::new(2).expect("revision two"),
    );
    let recovered = gateway
        .recover_git_mutation(
            prior_binding,
            rollback_ids.action,
            action_digest,
            payload_digest,
            &artifacts,
        )
        .expect("recover rollback after artifact quota failure");
    let GitMutationRecoveryOutcome::Rollback(recovered) = recovered else {
        panic!("rollback receipt was not recovered");
    };
    assert_eq!(recovered.snapshot().manifest().snapshot_id(), rollback_snapshot);
    assert_eq!(gateway.state().revision(), RevisionNumber::new(3).expect("rollback revision"));
}
