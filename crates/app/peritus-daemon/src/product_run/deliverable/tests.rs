use super::*;
use crate::product_run::ProductRunRequest;
use peritus_app_protocol::ProductProviderSelection;
use peritus_provider_core::CancellationToken;
use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceDependencies, EvidenceRecord,
    EvidenceStatus, QualificationEvidence, RunDisposition, SettlementCause, SettlementReducer,
};
use peritus_types::{ProviderProfileId, Sha256Digest, WorkspaceId};
use std::process::Output;
use std::sync::{Arc, atomic::AtomicBool};
use tempfile::TempDir;

#[path = "tests/commit_handoff.rs"]
mod commit_handoff;
#[path = "tests/discard.rs"]
mod discard;
#[path = "tests/nested.rs"]
mod nested;

#[test]
fn export_and_discard_are_limited_to_exact_deliverable_paths() {
    let repository = repository();
    let chosen_path = repository.path().join("chosen.txt");
    let chosen_baseline = fs::read(&chosen_path).expect("chosen baseline");
    fs::write(&chosen_path, "changed\n").expect("chosen change");
    fs::write(repository.path().join("unrelated.txt"), "unrelated change\n")
        .expect("unrelated change");
    fs::write(repository.path().join("new.txt"), "new file\n").expect("new file");
    let deliverable = ProductDeliverable::candidate(
        repository.path().to_string_lossy().into_owned(),
        vec!["chosen.txt".to_owned(), "new.txt".to_owned()],
        vec!["cargo test --manifest-path game/Cargo.toml".to_owned()],
        "cargo run --manifest-path game/Cargo.toml".to_owned(),
        CandidateStage::Qualified,
    )
    .expect("deliverable");

    let patch = String::from_utf8(
        uncommitted_patch(repository.path(), deliverable.changed_paths()).expect("patch"),
    )
    .expect("UTF-8 patch");
    assert!(patch.contains("chosen.txt"));
    assert!(patch.contains("new.txt"));
    assert!(!patch.contains("unrelated.txt"));

    discard_deliverable(&deliverable).expect("discard");
    assert_eq!(fs::read(chosen_path).expect("chosen"), chosen_baseline);
    assert!(!repository.path().join("new.txt").exists());
    assert_eq!(
        fs::read_to_string(repository.path().join("unrelated.txt")).expect("unrelated"),
        "unrelated change\n"
    );
}

#[test]
fn commit_excludes_an_unrelated_pre_staged_change() {
    let repository = repository();
    fs::write(repository.path().join("chosen.txt"), "changed\n").expect("chosen change");
    fs::write(repository.path().join("unrelated.txt"), "unrelated change\n")
        .expect("unrelated change");
    git(repository.path(), &["add", "--", "unrelated.txt"]);
    let deliverable = ProductDeliverable::candidate(
        repository.path().to_string_lossy().into_owned(),
        vec!["chosen.txt".to_owned()],
        vec!["cargo test --manifest-path game/Cargo.toml".to_owned()],
        "cargo run --manifest-path game/Cargo.toml".to_owned(),
        CandidateStage::Qualified,
    )
    .expect("deliverable");

    let revision = commit_deliverable(&deliverable, "finish the game").expect("commit");
    assert_eq!(revision.len(), 40);
    let committed = git_output(repository.path(), &["show", "--format=", "--name-only", "HEAD"]);
    assert!(committed.contains("chosen.txt"));
    assert!(!committed.contains("unrelated.txt"));
    let staged = git_output(repository.path(), &["diff", "--cached", "--name-only"]);
    assert_eq!(staged.trim(), "unrelated.txt");
}

#[test]
fn candidate_action_rejects_a_workspace_that_no_longer_matches_its_digest() {
    let repository = repository();
    fs::write(repository.path().join("chosen.txt"), "candidate\n").expect("candidate");
    let record = candidate_record(&repository);
    let deliverable = record.snapshot.deliverable().expect("deliverable");

    validate_exact_candidate(&record, deliverable, repository.path()).expect("current candidate");
    fs::write(repository.path().join("chosen.txt"), "different candidate\n")
        .expect("change candidate");

    assert_eq!(
        validate_exact_candidate(&record, deliverable, repository.path()),
        Err(ProductRunServiceError::InvalidState),
    );
}

#[test]
fn bracketed_filename_does_not_commit_or_discard_a_matching_unrelated_file() {
    let repository = repository();
    let root = repository.path();
    let chosen = "item[ab].txt";
    let unrelated = "itema.txt";
    fs::write(root.join(chosen), b"base\n").unwrap();
    fs::write(root.join(unrelated), b"base\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "bracket names"]);
    fs::write(root.join(chosen), b"task change\n").unwrap();
    fs::write(root.join(unrelated), b"unrelated change\n").unwrap();
    let deliverable = ProductDeliverable::candidate(
        root.to_string_lossy().into_owned(),
        vec![chosen.to_owned()],
        vec!["true".to_owned()],
        "inspect".to_owned(),
        CandidateStage::Qualified,
    )
    .unwrap();
    let patch =
        String::from_utf8(uncommitted_patch(root, deliverable.changed_paths()).unwrap()).unwrap();
    assert!(!patch.contains("itema.txt"));
    discard_deliverable(&deliverable).unwrap();
    assert_eq!(fs::read(root.join(chosen)).unwrap(), b"base\n");
    assert_eq!(fs::read(root.join(unrelated)).unwrap(), b"unrelated change\n");
    fs::write(root.join(chosen), b"task change\n").unwrap();
    commit_deliverable(&deliverable, "commit brackets").unwrap();
    let committed = git_output(root, &["show", "--format=", "--name-only", "HEAD"]);
    assert!(committed.contains(chosen));
    assert!(!committed.contains(unrelated));
    assert_eq!(git_output(root, &["show", "HEAD:itema.txt"]), "base\n");
}

#[test]
fn repeated_user_action_is_idempotent() {
    let repository = repository();
    fs::write(repository.path().join("chosen.txt"), "candidate\n").expect("candidate");
    let mut record = candidate_record(&repository);
    let accepted = record.snapshot.deliverable().expect("deliverable").clone().mark_accepted();
    record.snapshot = record.snapshot.clone().with_deliverable(accepted.clone());

    assert!(repeated_action(&record, ProductRunControlAction::Accept, &accepted).is_some());
    assert!(repeated_action(&record, ProductRunControlAction::Commit, &accepted).is_none());
}

#[test]
fn many_recovery_locations_cannot_make_a_successful_discard_unreportable() {
    let repository = repository();
    let record = candidate_record(&repository);
    let paths = (0..512)
        .map(|index| PathBuf::from(format!("{}/{index}", "a".repeat(4096))))
        .collect::<Vec<_>>();
    let status = discard_status(&paths);
    assert!(status.contains("additional recovery records"));
    let snapshot = replace_snapshot(
        &record.snapshot,
        record.snapshot.phase(),
        &status,
        record.snapshot.summary(),
    )
    .expect("bounded successful discard status");
    assert!(snapshot.status().starts_with("Deliverable discarded"));
    assert_eq!(discard_status(&[]), "Deliverable discarded");
}

#[test]
fn restart_marks_changed_candidate_evidence_stale() {
    let repository = repository();
    let state = TempDir::new().expect("state");
    let saved_patch = state.path().join("saved-original-candidate.patch");
    fs::write(&saved_patch, b"retained original source patch\n").unwrap();
    fs::write(repository.path().join("chosen.txt"), "candidate\n").expect("candidate");
    let mut record = qualified_record(candidate_record(&repository));
    let exported = record
        .snapshot
        .deliverable()
        .unwrap()
        .clone()
        .mark_exported(saved_patch.to_string_lossy().into_owned())
        .unwrap();
    record.snapshot = record.snapshot.clone().with_deliverable(exported);
    let run_id = record.request.run_id();
    let workspace_id = record.request.workspace_id();
    let directory = state.path().join("product-runs");
    fs::create_dir_all(&directory).expect("product run directory");
    let mut records = std::collections::BTreeMap::from([(run_id, record)]);
    let workspaces =
        std::collections::BTreeMap::from([(workspace_id, repository.path().to_owned())]);
    fs::write(repository.path().join("chosen.txt"), "changed after restart\n")
        .expect("changed candidate");

    crate::product_run::recovery::reconcile_restored_candidates(
        &directory,
        &mut records,
        &workspaces,
    )
    .expect("reconcile candidate");

    let record = records.get(&run_id).expect("reconciled record");
    let checkpoint = record.checkpoint.expect("checkpoint");
    assert_eq!(record.snapshot.phase(), peritus_app_protocol::ProductRunPhase::Failed);
    assert_eq!(checkpoint.stage(), CandidateStage::Changed);
    assert!(matches!(checkpoint.gates(), EvidenceStatus::Stale(_)));
    assert!(matches!(checkpoint.obligations(), EvidenceStatus::Stale(_)));
    assert!(matches!(checkpoint.review(), EvidenceStatus::Stale(_)));
    assert_eq!(
        record.settlement.expect("settlement").disposition(),
        RunDisposition::CandidateAvailable,
    );
    assert_eq!(
        record.snapshot.deliverable().expect("deliverable").qualification(),
        CandidateStage::Changed,
    );
    assert!(!record.snapshot.deliverable().expect("deliverable").accepted());
    assert_eq!(record.snapshot.deliverable().unwrap().export_path(), saved_patch.to_string_lossy());
    assert!(
        repeated_action(
            record,
            ProductRunControlAction::Export,
            record.snapshot.deliverable().unwrap()
        )
        .is_some()
    );
    assert!(record.resume.is_none());
    assert_eq!(
        validate_exact_candidate(
            record,
            record.snapshot.deliverable().expect("deliverable"),
            repository.path(),
        ),
        Err(ProductRunServiceError::InvalidState),
    );
}

fn qualified_record(mut record: crate::product_run::RunRecord) -> crate::product_run::RunRecord {
    let identity = *record.checkpoint.as_ref().expect("checkpoint").identity();
    let positive = |dependencies| {
        EvidenceStatus::Current(EvidenceRecord::new(
            identity,
            dependencies,
            QualificationEvidence::Satisfied,
        ))
    };
    let checkpoint = CandidateCheckpoint::new(
        identity,
        CandidateStage::Qualified,
        positive(EvidenceDependencies::GATES),
        positive(EvidenceDependencies::OBLIGATIONS),
        positive(EvidenceDependencies::REVIEW),
    )
    .expect("qualified checkpoint");
    let mut reducer = SettlementReducer::new();
    reducer.observe(checkpoint).expect("observe checkpoint");
    record.settlement = Some(reducer.settle(SettlementCause::Completed).expect("settlement"));
    record.checkpoint = Some(checkpoint);
    let deliverable = record.snapshot.deliverable().expect("deliverable");
    let qualified = ProductDeliverable::candidate(
        deliverable.workspace_path().to_owned(),
        deliverable.changed_paths().to_vec(),
        deliverable.successful_commands().to_vec(),
        deliverable.run_instructions().to_owned(),
        CandidateStage::Qualified,
    )
    .expect("qualified deliverable");
    record.snapshot = replace_snapshot(
        &record.snapshot,
        peritus_app_protocol::ProductRunPhase::Complete,
        "Accepted",
        "Qualified",
    )
    .expect("complete snapshot")
    .with_deliverable(qualified);
    record
}

fn candidate_record(repository: &TempDir) -> crate::product_run::RunRecord {
    let run_id = RunId::new([41; 16]).expect("run");
    let workspace_id = WorkspaceId::new([42; 16]).expect("workspace");
    let profile = ProviderProfileId::new([43; 16]).expect("provider");
    let providers = ProductProviderSelection::new(profile, profile, profile);
    let request = ProductRunRequest::new(run_id, workspace_id, providers, "finish game".to_owned())
        .expect("request");
    let repository_digest = ProductRunner::candidate_digest(repository.path()).expect("digest");
    let content_digest =
        ProductRunner::candidate_source_digest(repository.path()).expect("source digest");
    let identity = CandidateIdentity::new(
        run_id,
        workspace_id,
        content_digest,
        repository_digest,
        Some(Sha256Digest::new([0xEC; 32])),
        1,
        1,
    )
    .expect("identity");
    let checkpoint = CandidateCheckpoint::new(
        identity,
        CandidateStage::Changed,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
    )
    .expect("checkpoint");
    let deliverable = ProductDeliverable::candidate(
        repository.path().to_string_lossy().into_owned(),
        vec!["chosen.txt".to_owned()],
        Vec::new(),
        "cargo run".to_owned(),
        CandidateStage::Changed,
    )
    .expect("deliverable");
    let snapshot = ProductRunSnapshot::new(
        run_id,
        workspace_id,
        providers,
        peritus_app_protocol::ProductRunPhase::Failed,
        1,
        "finish game".to_owned(),
        "Candidate available".to_owned(),
        "diff".to_owned(),
        String::new(),
        String::new(),
        "remaining work".to_owned(),
        super::super::operation::retained_execution(
            run_id,
            peritus_app_protocol::ProductRunPhase::Failed,
            "",
        )
        .expect("operation"),
    )
    .expect("snapshot")
    .with_deliverable(deliverable);
    crate::product_run::RunRecord {
        interaction: crate::product_run::interaction::InteractionOptions::test_for_run(
            peritus_app_protocol::ProductInteractionMode::Build,
            peritus_app_protocol::ProductRoleModels::default(),
            run_id,
            workspace_id,
        ),
        attempt_admission: None,
        continuation_admissions: Vec::new(),
        continuation_sources: Vec::new(),
        request,
        snapshot,
        cancelled: Arc::new(AtomicBool::new(false)),
        control_cancellation: peritus_journal::JournalCancellation::new(),
        user_cancelled: false,
        provider_cancellation: CancellationToken::new(),
        finding_state: String::new(),
        progress: crate::product_run::RunProgress::default(),
        checkpoint: Some(checkpoint),
        settlement: None,
        resume: None,
        remaining_work: vec!["run exact checks".to_owned()],
        interruption_cause: "reviewer unavailable".to_owned(),
        candidate_actionable: true,
        task_baseline_required: false,
        task_baseline: None,
        preview: crate::product_run::PreviewAggregate::default(),
    }
}

fn repository() -> TempDir {
    let repository = TempDir::new().expect("temporary repository");
    git(repository.path(), &["init", "--quiet"]);
    git(repository.path(), &["config", "user.name", "Peritus Test"]);
    git(repository.path(), &["config", "user.email", "peritus@example.invalid"]);
    git(repository.path(), &["config", "commit.gpgsign", "false"]);
    git(repository.path(), &["config", "core.autocrlf", "false"]);
    fs::write(repository.path().join("chosen.txt"), "base\n").expect("chosen base");
    fs::write(repository.path().join("unrelated.txt"), "base\n").expect("unrelated base");
    git(repository.path(), &["add", "--", "chosen.txt", "unrelated.txt"]);
    git(repository.path(), &["commit", "--quiet", "-m", "initial"]);
    repository
}

fn git(root: &Path, arguments: &[&str]) {
    let output = Command::new("git").args(arguments).current_dir(root).output().expect("git");
    assert_success(output);
}

fn git_output(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git").args(arguments).current_dir(root).output().expect("git");
    assert_success(output.clone());
    String::from_utf8(output.stdout).expect("UTF-8 git output")
}

fn assert_success(output: Output) {
    assert!(output.status.success(), "git failed: {}", String::from_utf8_lossy(&output.stderr));
}
