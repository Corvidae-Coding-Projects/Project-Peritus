//! Candidate capture and source-revision regression tests.

use std::{fs, process::Command};

use super::*;

#[test]
fn retry_preserves_a_clean_review_when_failed_gates_are_reacquired() {
    let root = repository();
    let baseline = CandidateBaseline::capture(root.path()).expect("baseline");
    let recorder =
        CandidateRecorder::new(root.path(), baseline, run_id(), workspace_id(), None, false)
            .expect("recorder");
    fs::write(root.path().join("candidate.txt"), "changed").expect("candidate");
    recorder
        .record(CandidateStage::SelfChecked, 1, CheckpointEvidence::Gates(false))
        .expect("failed gates");
    recorder
        .record(CandidateStage::SelfChecked, 1, CheckpointEvidence::Review(true))
        .expect("clean review");
    recorder
        .record(CandidateStage::GatesPassed, 1, CheckpointEvidence::Gates(true))
        .expect("reacquired gates");
    recorder.record_pending_review(1).expect("start another review");
    let checkpoint = recorder.checkpoint().expect("state").expect("candidate");
    assert_eq!(checkpoint.stage(), CandidateStage::GatesPassed);
    assert!(checkpoint.review().is_current_and_satisfied(checkpoint.identity()));
    assert!(!checkpoint.obligations().is_current_and_satisfied(checkpoint.identity()));
}

#[test]
fn changed_candidate_stales_prior_gate_evidence() {
    let root = repository();
    let baseline = CandidateBaseline::capture(root.path()).expect("baseline");
    let recorder =
        CandidateRecorder::new(root.path(), baseline, run_id(), workspace_id(), None, false)
            .expect("recorder");
    fs::write(root.path().join("candidate.txt"), "first").expect("first mutation");
    recorder
        .record(CandidateStage::GatesPassed, 1, CheckpointEvidence::Gates(true))
        .expect("gates");
    fs::write(root.path().join("candidate.txt"), "second").expect("second mutation");

    let checkpoint = recorder
        .record(CandidateStage::Changed, 1, CheckpointEvidence::None)
        .expect("mutation")
        .expect("candidate");

    assert!(matches!(checkpoint.gates(), EvidenceStatus::Stale(_)));
    assert_eq!(checkpoint.stage(), CandidateStage::Changed);
}

#[test]
fn conversation_revision_stales_prior_evidence() {
    let root = repository();
    let baseline = CandidateBaseline::capture(root.path()).expect("baseline");
    let recorder =
        CandidateRecorder::new(root.path(), baseline, run_id(), workspace_id(), None, false)
            .expect("recorder");
    fs::write(root.path().join("candidate.txt"), "changed").expect("mutation");
    recorder
        .record(CandidateStage::GatesPassed, 1, CheckpointEvidence::Gates(true))
        .expect("gates");

    let checkpoint = recorder.refresh(2).expect("refresh").expect("candidate");

    assert!(matches!(checkpoint.gates(), EvidenceStatus::Stale(_)));
    assert_eq!(checkpoint.identity().conversation_revision(), 2);
}

#[test]
fn fresh_reversion_clears_an_old_workspace_candidate() {
    let root = repository();
    let baseline = CandidateBaseline::capture(root.path()).expect("baseline");
    let recorder =
        CandidateRecorder::new(root.path(), baseline, run_id(), workspace_id(), None, false)
            .expect("recorder");
    fs::write(root.path().join("candidate.txt"), "changed").expect("mutation");
    assert!(recorder.refresh(1).expect("changed refresh").is_some());

    fs::write(root.path().join("candidate.txt"), "baseline").expect("reversion");

    assert!(recorder.refresh(1).expect("reverted refresh").is_none());
    assert!(recorder.checkpoint().expect("checkpoint").is_none());
}

fn repository() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("root");
    run(root.path(), &["init", "--quiet"]);
    run(root.path(), &["config", "user.email", "peritus@example.invalid"]);
    run(root.path(), &["config", "user.name", "Peritus Test"]);
    fs::write(root.path().join("candidate.txt"), "baseline").expect("baseline file");
    run(root.path(), &["add", "."]);
    run(root.path(), &["commit", "--quiet", "-m", "fixture"]);
    root
}

fn run(root: &Path, arguments: &[&str]) {
    assert!(Command::new("git").args(arguments).current_dir(root).status().unwrap().success());
}

fn run_id() -> RunId {
    RunId::new([1; 16]).expect("run id")
}

fn workspace_id() -> WorkspaceId {
    WorkspaceId::new([2; 16]).expect("workspace id")
}
