use super::*;
use peritus_app_protocol::ProductRunSettlementSnapshot;

#[test]
fn legacy_automated_acceptance_wording_does_not_create_a_human_decision() {
    let repository = repository();
    fs::write(repository.path().join("chosen.txt"), b"candidate source\n").unwrap();
    let mut record = qualified_record(candidate_record(&repository));
    record.snapshot = replace_snapshot(
        &record.snapshot,
        record.snapshot.phase(),
        "Accepted — passing checks and independent review",
        record.snapshot.summary(),
    )
    .unwrap();
    let state = TempDir::new().unwrap();
    let directory = state.path().join("product-runs");
    fs::create_dir(&directory).unwrap();
    persist_record(&directory, &record).unwrap();
    let restored = crate::product_run::persistence::load_records(&directory).unwrap();
    let current = &restored[&record.request.run_id()];
    assert_eq!(current.snapshot.status(), "Qualified — passing checks and independent review");
    assert!(!current.snapshot.deliverable().unwrap().accepted());
    assert_eq!(current.checkpoint, record.checkpoint);
}

#[test]
fn committed_handoff_binds_current_files_without_reissuing_old_qualification() {
    let repository = repository();
    let root = repository.path();
    fs::write(root.join("chosen.txt"), b"candidate source\n").unwrap();
    fs::write(root.join("unrelated.txt"), b"human staged source\n").unwrap();
    git(root, &["add", "unrelated.txt"]);
    fs::write(root.join("human-draft.txt"), b"human untracked source\n").unwrap();
    let mut record = qualified_record(candidate_record(&repository));
    let original = record.checkpoint.unwrap();
    let state = TempDir::new().unwrap();
    let directory = state.path().join("product-runs");
    fs::create_dir(&directory).unwrap();
    let deliverable = record.snapshot.deliverable().unwrap().clone();
    let (committed, status) = commit::with_recovery(&directory, &mut record, deliverable).unwrap();
    record.snapshot = replace_snapshot(
        &record.snapshot,
        record.snapshot.phase(),
        &status,
        record.snapshot.summary(),
    )
    .unwrap()
    .with_deliverable(committed);
    persist_record(&directory, &record).unwrap();

    let checkpoint = record.checkpoint.unwrap();
    assert_eq!(
        checkpoint.identity().candidate_digest(),
        ProductRunner::candidate_digest(root).unwrap()
    );
    assert_ne!(checkpoint.identity().candidate_digest(), original.identity().candidate_digest());
    assert_eq!(
        checkpoint.identity().checkpoint_sequence(),
        original.identity().checkpoint_sequence() + 1
    );
    assert_eq!(checkpoint.stage(), CandidateStage::Changed);
    for evidence in [checkpoint.gates(), checkpoint.obligations(), checkpoint.review()] {
        assert!(matches!(evidence, EvidenceStatus::Stale(_)));
        assert_eq!(evidence.record().unwrap().provenance(), original.identity());
    }
    assert_eq!(record.snapshot.deliverable().unwrap().qualification(), checkpoint.stage());
    assert!(record.snapshot.deliverable().unwrap().accepted());
    assert!(record.candidate_actionable);
    assert_eq!(git_output(root, &["diff", "--cached", "--name-only"]).trim(), "unrelated.txt");
    assert_eq!(fs::read(root.join("human-draft.txt")).unwrap(), b"human untracked source\n");
    ProductRunSettlementSnapshot::new(record.snapshot.clone(), record.settlement.unwrap()).unwrap();

    let mut restored = crate::product_run::persistence::load_records(&directory).unwrap();
    let workspaces =
        std::collections::BTreeMap::from([(record.request.workspace_id(), root.to_path_buf())]);
    crate::product_run::recovery::reconcile_restored_candidates(
        &directory,
        &mut restored,
        &workspaces,
    )
    .unwrap();
    assert_eq!(restored[&record.request.run_id()].checkpoint, Some(checkpoint));
    git(root, &["reset", "--quiet", "HEAD", "--", "unrelated.txt"]);
    // The established Run fingerprint covers source and HEADs, independent of
    // staging-only choices that do not change the executable files.
    assert_eq!(
        ProductRunner::candidate_digest(root).unwrap(),
        checkpoint.identity().candidate_digest()
    );
    git(root, &["add", "unrelated.txt"]);
    assert_eq!(
        ProductRunner::candidate_digest(root).unwrap(),
        checkpoint.identity().candidate_digest()
    );
    fs::write(root.join("chosen.txt"), b"new human edit\n").unwrap();
    crate::product_run::recovery::reconcile_restored_candidates(
        &directory,
        &mut restored,
        &workspaces,
    )
    .unwrap();
    assert_eq!(restored[&record.request.run_id()].checkpoint, Some(checkpoint));
    assert_ne!(
        ProductRunner::candidate_digest(root).unwrap(),
        checkpoint.identity().candidate_digest()
    );
}

#[test]
fn nested_commit_binds_the_final_root_and_nested_git_state() {
    let repository = repository();
    let root = repository.path();
    let nested = root.join("application");
    fs::rename(super::repository().keep(), &nested).unwrap();
    fs::write(nested.join("chosen.txt"), b"nested candidate\n").unwrap();
    fs::write(nested.join("unrelated.txt"), b"nested human staged source\n").unwrap();
    git(&nested, &["add", "unrelated.txt"]);
    let mut record = qualified_record(candidate_record(&repository));
    let original = record.checkpoint.unwrap();
    let deliverable = ProductDeliverable::new(
        root.to_string_lossy().into_owned(),
        vec!["application".to_owned(), "application/chosen.txt".to_owned()],
        vec!["true".to_owned()],
        "inspect".to_owned(),
    )
    .unwrap();
    record.snapshot = record.snapshot.clone().with_deliverable(deliverable.clone());
    let state = TempDir::new().unwrap();
    let directory = state.path().join("product-runs");
    fs::create_dir(&directory).unwrap();
    let (committed, _) = commit::with_recovery(&directory, &mut record, deliverable).unwrap();
    let checkpoint = record.checkpoint.unwrap();
    assert_eq!(
        checkpoint.identity().candidate_digest(),
        ProductRunner::candidate_digest(root).unwrap()
    );
    assert_ne!(checkpoint.identity().candidate_digest(), original.identity().candidate_digest());
    assert_eq!(committed.qualification(), CandidateStage::Changed);
    assert_eq!(git_output(&nested, &["diff", "--cached", "--name-only"]).trim(), "unrelated.txt");
    git(&nested, &["commit", "--only", "-qm", "later human commit", "--", "unrelated.txt"]);
    assert_ne!(
        ProductRunner::candidate_digest(root).unwrap(),
        checkpoint.identity().candidate_digest()
    );
}

#[cfg(unix)]
#[test]
fn successful_commit_hook_edit_is_preserved_and_does_not_become_the_recorded_candidate() {
    use std::os::unix::fs::PermissionsExt as _;

    let repository = repository();
    let root = repository.path();
    fs::write(root.join("chosen.txt"), b"candidate source\n").unwrap();
    let mut record = qualified_record(candidate_record(&repository));
    let original = record.checkpoint.unwrap();
    let hook = root.join(".git/hooks/post-commit");
    fs::write(&hook, b"#!/bin/sh\nprintf 'human hook edit\\n' > chosen.txt\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
    let state = TempDir::new().unwrap();
    let directory = state.path().join("product-runs");
    fs::create_dir(&directory).unwrap();
    let deliverable = record.snapshot.deliverable().unwrap().clone();
    let (committed, status) = commit::with_recovery(&directory, &mut record, deliverable).unwrap();
    assert!(!committed.commit_revision().is_empty());
    assert!(status.contains("changed during commit"));
    assert_eq!(record.checkpoint, Some(original));
    assert_eq!(fs::read(root.join("chosen.txt")).unwrap(), b"human hook edit\n");
    assert_eq!(git_output(root, &["show", "HEAD:chosen.txt"]), "candidate source\n");
    assert_ne!(
        ProductRunner::candidate_digest(root).unwrap(),
        original.identity().candidate_digest()
    );
}
