use super::*;
use crate::trust::manifest_model::{
    ProofImpactDocument, ProofImpactEvidence, ProofImpactFindingSeverity, ProofImpactKind,
    ProofImpactPackage, ProofImpactSnapshot, ProofImpactVerdictArtifact,
    ProofImpactVerdictArtifactKind, ProofImpactVerdictArtifactRef, ProofImpactVerdictRef,
    ProofSourceChange,
};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "tests/git_bindings.rs"]
mod git_bindings;
#[path = "tests/metadata.rs"]
mod metadata;

static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);
pub(super) const SOURCE: &[u8] = b"pub fn reviewed() -> u64 { 7 }\n";

pub(super) struct GitFixture {
    pub(super) root: PathBuf,
    pub(super) authorization: String,
    pub(super) implementation: String,
    pub(super) implementation_tree: String,
}

impl GitFixture {
    pub(super) fn new() -> Self {
        let id = FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let root = env::temp_dir().join(format!("peritus-verdict-{}-{id}", std::process::id()));
        fs::create_dir_all(&root).expect("fixture root");
        command(&root, &["init", "--quiet"]);
        command(&root, &["config", "user.name", "Peritus Test"]);
        command(&root, &["config", "user.email", "peritus-test@example.invalid"]);
        command(&root, &["config", "commit.gpgsign", "false"]);
        fs::write(root.join("base.txt"), b"authorization base\n").expect("base file");
        command(&root, &["add", "base.txt"]);
        command(&root, &["commit", "--quiet", "-m", "authorization"]);
        let authorization = stdout(&root, &["rev-parse", "HEAD"]);
        fs::create_dir_all(root.join("crate/src")).expect("source directory");
        fs::write(root.join("crate/src/lib.rs"), SOURCE).expect("implementation source");
        command(&root, &["add", "crate/src/lib.rs"]);
        command(&root, &["commit", "--quiet", "-m", "implementation"]);
        let implementation = stdout(&root, &["rev-parse", "HEAD"]);
        let implementation_tree = stdout(&root, &["rev-parse", "HEAD^{tree}"]);
        Self { root, authorization, implementation, implementation_tree }
    }

    pub(super) fn write_artifact(
        &self,
        relative: &str,
        bytes: &[u8],
    ) -> ProofImpactVerdictArtifactRef {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().expect("artifact parent")).expect("artifact directory");
        fs::write(path, bytes).expect("artifact bytes");
        ProofImpactVerdictArtifactRef { path: relative.to_owned(), sha256: sha256_hex(bytes) }
    }

    pub(super) fn unrelated_commit(&self) -> String {
        stdout(&self.root, &["commit-tree", &self.implementation_tree, "-m", "unrelated"])
    }

    pub(super) fn blob(&self) -> String {
        stdout(&self.root, &["hash-object", "crate/src/lib.rs"])
    }
}

impl Drop for GitFixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.root);
    }
}

pub(super) fn command(root: &Path, arguments: &[&str]) -> Output {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .expect("Git fixture command");
    assert!(output.status.success(), "Git failed: {}", String::from_utf8_lossy(&output.stderr));
    output
}

pub(super) fn stdout(root: &Path, arguments: &[&str]) -> String {
    String::from_utf8(command(root, arguments).stdout)
        .expect("Git output must be UTF-8")
        .trim()
        .to_owned()
}

pub(super) fn repeated(character: char, count: usize) -> String {
    std::iter::repeat_n(character, count).collect()
}

pub(super) fn fixture_change() -> ProofImpactChange {
    ProofImpactChange {
        id: "PCR-0005".to_owned(),
        status: ProofImpactStatus::Approved,
        change_kinds: vec![
            ProofImpactKind::Executable,
            ProofImpactKind::Specification,
            ProofImpactKind::Precondition,
            ProofImpactKind::Postcondition,
            ProofImpactKind::Proof,
        ],
        source_changes: vec![ProofSourceChange {
            source_file: "crate/src/lib.rs".to_owned(),
            previous: None,
            current: Some(ProofImpactSnapshot {
                sha256: sha256_hex(SOURCE),
                affected_packages: vec![ProofImpactPackage {
                    package: "crate".to_owned(),
                    verification_class: "V".to_owned(),
                }],
            }),
        }],
        rationale: "reviewed transition".to_owned(),
        impact: "reviewed conservative impact".to_owned(),
        evidence: vec![ProofImpactEvidence {
            kind: ProofImpactEvidenceKind::VerusVerify,
            owning_crate: "crate".to_owned(),
            command: "cargo verus verify --package crate --locked".to_owned(),
        }],
        owner: "ACTOR-0001".to_owned(),
        reviewer: "ACTOR-0003".to_owned(),
        review_date: "2026-08-22".to_owned(),
        verdict: Some(ProofImpactVerdictRef {
            path: "verification/reviews/PCR-0005.toml".to_owned(),
            sha256: repeated('b', 64),
        }),
    }
}

pub(super) fn artifact(
    kind: ProofImpactVerdictArtifactKind,
    reference: &ProofImpactVerdictArtifactRef,
) -> ProofImpactVerdictArtifact {
    ProofImpactVerdictArtifact {
        kind,
        path: reference.path.clone(),
        sha256: reference.sha256.clone(),
    }
}

pub(super) fn verdict(fixture: &GitFixture, change: &ProofImpactChange) -> ProofImpactVerdict {
    let report = fixture.write_artifact(
        "verification/reviews/PCR-0005/review-report.md",
        b"Independent review scope, procedure, and complete finding ledger.\n",
    );
    let gate_output = fixture.write_artifact(
        "verification/reviews/PCR-0005/gates/verus-verify.txt",
        b"verification results:: 1 verified, 0 errors\n",
    );
    let detail = fixture.write_artifact(
        "verification/reviews/PCR-0005/findings/FINDING-0001-detail.md",
        b"The mutation was detected.\n",
    );
    let evidence = fixture.write_artifact(
        "verification/reviews/PCR-0005/findings/FINDING-0001-evidence.txt",
        b"mutation test: passed\n",
    );
    let gate_evidence = vec![ProofImpactVerdictGate {
        kind: ProofImpactEvidenceKind::VerusVerify,
        owning_crate: "crate".to_owned(),
        command: "cargo verus verify --package crate --locked".to_owned(),
        result: ProofImpactGateResult::Passed,
        output: gate_output.clone(),
    }];
    let findings = vec![ProofImpactVerdictFinding {
        id: "FINDING-0001".to_owned(),
        severity: ProofImpactFindingSeverity::High,
        blocking: true,
        disposition: ProofImpactFindingDisposition::Fixed,
        detail: detail.clone(),
        evidence: evidence.clone(),
    }];
    let artifacts = vec![
        artifact(ProofImpactVerdictArtifactKind::ReviewReport, &report),
        artifact(ProofImpactVerdictArtifactKind::GateOutput, &gate_output),
        artifact(ProofImpactVerdictArtifactKind::FindingDetail, &detail),
        artifact(ProofImpactVerdictArtifactKind::FindingEvidence, &evidence),
    ];
    ProofImpactVerdict {
        schema: SCHEMA.to_owned(),
        schema_version: 1,
        id: "VERDICT-PCR-0005".to_owned(),
        pcr_id: change.id.clone(),
        reviewer: change.reviewer.clone(),
        reviewer_principal: "reviewer-principal".to_owned(),
        authorization_base_commit: fixture.authorization.clone(),
        implementation_commit: fixture.implementation.clone(),
        implementation_tree: fixture.implementation_tree.clone(),
        source_transitions_sha256: digest::source_transitions(change),
        gate_evidence_sha256: digest::gate_evidence(&gate_evidence),
        finding_set_sha256: digest::findings(&findings),
        artifact_inventory_sha256: digest::artifact_inventory(&artifacts),
        decision: ProofImpactVerdictDecision::Approved,
        reviewed_at: "2026-08-22T22:17:03.123456789Z".to_owned(),
        review_report: report,
        gate_evidence,
        findings,
        artifacts,
    }
}

pub(super) fn diagnostics(
    fixture: &GitFixture,
    change: &ProofImpactChange,
    verdict: &ProofImpactVerdict,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    validate_artifact(
        &fixture.root,
        Path::new("verification/reviews/PCR-0005.toml"),
        change,
        Some("reviewer-principal"),
        verdict,
        &mut diagnostics,
    );
    diagnostics
}
