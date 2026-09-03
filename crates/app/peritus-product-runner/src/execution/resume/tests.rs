use std::{fs, path::Path, process::Command};

use super::*;
use peritus_run_settlement::{CandidateStage, EvidenceStatus};
use peritus_types::{RunId, WorkspaceId};

#[test]
fn provider_retry_reuses_valid_design_and_resumes_review() {
    let checkpoint = checkpoint(1, 7);
    let resume = fixture(&checkpoint, ProductRunPhase::Reviewing);
    assert_eq!(
        resume.plan(*checkpoint.identity(), "User:\nBuild it.").expect("plan"),
        ProductRunPhase::Reviewing,
    );
}

#[test]
fn conversation_change_restarts_at_design() {
    let checkpoint = checkpoint(1, 7);
    let resume = fixture(&checkpoint, ProductRunPhase::Reviewing);
    let changed = CandidateIdentity::new(
        checkpoint.identity().run_id(),
        checkpoint.identity().workspace_id(),
        checkpoint.identity().candidate_digest(),
        2,
        8,
    )
    .expect("changed conversation");
    assert_eq!(
        resume.plan(changed, "User:\nBuild it differently.").expect("plan"),
        ProductRunPhase::Designing,
    );
}

#[test]
fn report_retry_reuses_every_completed_execution_phase() {
    let checkpoint = checkpoint(1, 7);
    let resume = fixture(&checkpoint, ProductRunPhase::Finalizing);
    assert_eq!(
        resume.plan(*checkpoint.identity(), "User:\nBuild it.").expect("plan"),
        ProductRunPhase::Finalizing,
    );
}

fn fixture(checkpoint: &CandidateCheckpoint, phase: ProductRunPhase) -> ProductRunResume {
    let repository = repository();
    let baseline = CandidateBaseline::capture(repository.path()).expect("baseline");
    ProductRunResume::capture(ResumeCapture {
        checkpoint: *checkpoint,
        baseline,
        next_phase: phase,
        design_path: PathBuf::from("run.design.md"),
        design_markdown: "# Design\n\n## Objective\nBuild it.\n".to_owned(),
        design_revision: 1,
        task_summary: "candidate".to_owned(),
        run_instructions: "cargo test".to_owned(),
        fix_summaries: Vec::new(),
        tool_calls: 1,
        finding_state: String::new(),
        diff: "candidate diff".to_owned(),
        gates: "gates pending".to_owned(),
        review: "review pending".to_owned(),
        gate_report: None,
        developer_evidence: "evidence".to_owned(),
        successful_commands: Vec::new(),
        fixer_cycles: 0,
        transcript: "User:\nBuild it.".to_owned(),
    })
    .expect("resume")
}

fn checkpoint(revision: u64, sequence: u64) -> CandidateCheckpoint {
    let identity = CandidateIdentity::new(
        RunId::new([1; 16]).expect("run"),
        WorkspaceId::new([2; 16]).expect("workspace"),
        Sha256Digest::new([3; 32]),
        revision,
        sequence,
    )
    .expect("identity");
    CandidateCheckpoint::new(
        identity,
        CandidateStage::Changed,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
    )
    .expect("checkpoint")
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
    assert!(Command::new("git").args(arguments).current_dir(root).status().expect("git").success());
}
