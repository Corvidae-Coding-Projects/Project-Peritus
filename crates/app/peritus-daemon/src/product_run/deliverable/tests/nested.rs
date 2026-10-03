use super::*;

#[test]
fn partial_nested_commit_retries_after_reconciliation_only_for_unchanged_source() {
    let repository = repository();
    let root = repository.path();
    let nested = root.join("application");
    fs::rename(super::repository().keep(), &nested).unwrap();
    fs::write(nested.join("chosen.txt"), b"candidate source\n").unwrap();
    let mut record = qualified_record(candidate_record(&repository));
    let deliverable = ProductDeliverable::candidate(
        root.to_string_lossy().into_owned(),
        vec!["application".to_owned(), "application/chosen.txt".to_owned()],
        vec!["true".to_owned()],
        "inspect".to_owned(),
        CandidateStage::Qualified,
    )
    .unwrap();
    record.snapshot = record.snapshot.clone().with_deliverable(deliverable.clone());
    let state = TempDir::new().unwrap();
    let directory = state.path().join("product-runs");
    fs::create_dir(&directory).unwrap();
    git(root, &["config", "commit.gpgsign", "true"]);
    git(root, &["config", "gpg.program", "peritus-test-missing-signer"]);
    assert!(commit::with_recovery(&directory, &mut record, deliverable).is_err());
    assert_eq!(git_output(&nested, &["show", "HEAD:chosen.txt"]), "candidate source\n");
    let nested_head = git_output(&nested, &["rev-parse", "HEAD"]);
    let original_patch = fs::read(record.snapshot.deliverable().unwrap().export_path()).unwrap();
    let run = record.request.run_id();
    let workspaces =
        std::collections::BTreeMap::from([(record.request.workspace_id(), root.to_path_buf())]);
    let mut records = crate::product_run::persistence::load_unchecked_records(&directory)
        .expect("restart persisted run");
    crate::product_run::recovery::reconcile_restored_candidates(
        &directory,
        &mut records,
        &workspaces,
    )
    .unwrap();
    let record = records.get_mut(&run).unwrap();
    let deliverable = record.snapshot.deliverable().unwrap().clone();
    assert!(!record.candidate_actionable);
    assert!(repeated_action(record, ProductRunControlAction::Export, &deliverable).is_some());
    fs::write(deliverable.export_path(), b"changed patch\n").unwrap();
    assert!(commit::validate_retry(&directory, record, &deliverable, root).is_err());
    fs::write(deliverable.export_path(), &original_patch).unwrap();
    fs::write(nested.join("chosen.txt"), b"new human edit\n").unwrap();
    assert!(commit::validate_retry(&directory, record, &deliverable, root).is_err());
    fs::write(nested.join("chosen.txt"), b"candidate source\n").unwrap();
    let retry = commit::validate_retry(&directory, record, &deliverable, root)
        .expect("retry retained source");
    assert_eq!(retry.qualification(), CandidateStage::SelfChecked);
    git(root, &["config", "commit.gpgsign", "false"]);
    let (committed, _) = commit::with_recovery(&directory, record, retry).unwrap();
    assert!(!committed.commit_revision().is_empty());
    assert_eq!(fs::read(committed.export_path()).unwrap(), original_patch);
    assert_eq!(git_output(&nested, &["rev-parse", "HEAD"]), nested_head);
}

#[test]
fn failed_signer_keeps_a_durable_source_patch_available() {
    let repository = repository();
    let root = repository.path();
    fs::write(root.join("chosen.txt"), b"recover this candidate\n").unwrap();
    let mut record = candidate_record(&repository);
    git(root, &["config", "commit.gpgsign", "true"]);
    git(root, &["config", "gpg.program", "peritus-test-missing-signer"]);
    let state = TempDir::new().unwrap();
    let directory = state.path().join("product-runs");
    fs::create_dir(&directory).unwrap();
    let deliverable = record.snapshot.deliverable().unwrap().clone();
    let error = commit::with_recovery(&directory, &mut record, deliverable).unwrap_err();
    assert!(matches!(
        error,
        ProductRunServiceError::Context { operation: "commit deliverable", .. }
    ));
    let deliverable = record.snapshot.deliverable().unwrap();
    let patch = fs::read(deliverable.export_path()).unwrap();
    assert!(String::from_utf8_lossy(&patch).contains("+recover this candidate"));
    assert!(repeated_action(&record, ProductRunControlAction::Export, deliverable).is_some());
    assert!(record.snapshot.status().contains("Commit did not complete"));
    let records = crate::product_run::persistence::record_directory(&directory).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(
        &fs::read(records.join(format!("{}.json", run_hex(record.request.run_id())))).unwrap(),
    )
    .unwrap();
    assert!(saved.to_string().contains("Commit did not complete"));
    git(root, &["config", "commit.gpgsign", "false"]);
    let deliverable = record.snapshot.deliverable().unwrap().clone();
    let (committed, _) = commit::with_recovery(&directory, &mut record, deliverable).unwrap();
    assert!(!committed.commit_revision().is_empty());
    assert_eq!(fs::read(committed.export_path()).unwrap(), patch);
}

#[test]
fn commit_handles_deletions_and_a_previously_committed_candidate() {
    let repository = repository();
    fs::remove_file(repository.path().join("chosen.txt")).unwrap();
    let deliverable = candidate_record(&repository).snapshot.deliverable().unwrap().clone();
    let first = commit_deliverable(&deliverable, "delete source").unwrap();
    assert!(
        !git_output(repository.path(), &["ls-tree", "--name-only", "HEAD"]).contains("chosen.txt")
    );
    let second = commit_deliverable(&deliverable, "already committed").unwrap();
    assert_eq!(first, second);
}

#[test]
fn commit_handles_a_deep_unborn_repository() {
    let repository = repository();
    let root = repository.path();
    let outer = root.join("outer");
    fs::rename(super::repository().keep(), &outer).unwrap();
    let inner = outer.join("inner");
    fs::create_dir(&inner).unwrap();
    git(&inner, &["init", "--quiet"]);
    git(&inner, &["config", "user.name", "Peritus Test"]);
    git(&inner, &["config", "user.email", "peritus@example.invalid"]);
    git(&inner, &["config", "commit.gpgsign", "false"]);
    fs::write(inner.join("source.txt"), b"first source\n").unwrap();
    let deliverable = ProductDeliverable::candidate(
        root.to_string_lossy().into_owned(),
        vec!["outer/inner/source.txt".to_owned()],
        vec!["true".to_owned()],
        "inspect".to_owned(),
        CandidateStage::Qualified,
    )
    .unwrap();
    commit_deliverable(&deliverable, "commit new application").unwrap();
    assert_eq!(git_output(&inner, &["show", "HEAD:source.txt"]), "first source\n");
    assert_eq!(
        git_output(root, &["rev-parse", "HEAD:outer"]),
        git_output(&outer, &["rev-parse", "HEAD"])
    );
    assert_eq!(
        git_output(&outer, &["rev-parse", "HEAD:inner"]),
        git_output(&inner, &["rev-parse", "HEAD"])
    );
}

#[test]
fn commit_saves_nested_source_and_preserves_unrelated_staged_changes() {
    let repository = repository();
    let root = repository.path();
    let child = super::repository();
    let nested = root.join("application");
    fs::rename(child.path(), &nested).unwrap();
    fs::write(nested.join("chosen.txt"), b"nested task change\n").unwrap();
    fs::write(nested.join("unrelated.txt"), b"nested user change\n").unwrap();
    git(&nested, &["add", "unrelated.txt"]);
    fs::write(root.join("unrelated.txt"), b"parent user change\n").unwrap();
    git(root, &["add", "unrelated.txt"]);
    let deliverable = ProductDeliverable::candidate(
        root.to_string_lossy().into_owned(),
        vec!["application".to_owned(), "application/chosen.txt".to_owned()],
        vec!["true".to_owned()],
        "inspect".to_owned(),
        CandidateStage::Qualified,
    )
    .unwrap();
    commit_deliverable(&deliverable, "save application").expect("commit nested deliverable");
    assert_eq!(git_output(&nested, &["show", "HEAD:chosen.txt"]), "nested task change\n");
    assert_eq!(git_output(&nested, &["show", "HEAD:unrelated.txt"]), "base\n");
    assert_eq!(git_output(root, &["show", "HEAD:unrelated.txt"]), "base\n");
    assert_eq!(git_output(&nested, &["diff", "--cached", "--name-only"]).trim(), "unrelated.txt");
    assert_eq!(git_output(root, &["diff", "--cached", "--name-only"]).trim(), "unrelated.txt");
    assert_eq!(
        git_output(root, &["rev-parse", "HEAD:application"]),
        git_output(&nested, &["rev-parse", "HEAD"])
    );
}
