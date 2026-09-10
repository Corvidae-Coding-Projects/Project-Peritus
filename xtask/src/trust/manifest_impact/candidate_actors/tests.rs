use super::*;
use crate::trust::manifest_actor_model::ActorsDocument;
use crate::trust::manifest_model::{
    ProofImpactPackage, ProofImpactSnapshot, ProofImpactStatus, ProofImpactVerdictArtifactRef,
    ProofImpactVerdictDecision,
};
use serde_json::json;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);
const REPOSITORY: &str = "Corvidae-Coding-Projects/Project-Peritus";

#[derive(Clone, Copy)]
struct ActorSpec {
    id: &'static str,
    name: &'static str,
    role: &'static str,
    session: u64,
    task: &'static str,
}

const BASE: &[ActorSpec] = &[
    ActorSpec {
        id: "ACTOR-0001",
        name: "Historical Owner",
        role: "owner",
        session: 11,
        task: "/root/base_owner",
    },
    ActorSpec {
        id: "ACTOR-0002",
        name: "Historical Reviewer",
        role: "reviewer",
        session: 12,
        task: "/root/base_review",
    },
];

const FRESH: &[ActorSpec] = &[
    ActorSpec {
        id: "ACTOR-0003",
        name: "Candidate Owner",
        role: "owner",
        session: 22,
        task: "/root/workbench_p4",
    },
    ActorSpec {
        id: "ACTOR-0004",
        name: "Independent Reviewer",
        role: "reviewer",
        session: 23,
        task: "/root/workbench_formal_review",
    },
];

struct Fixture {
    root: PathBuf,
    protected: ActorsDocument,
    base_actor_bytes: Vec<u8>,
    base_provenance_bytes: Vec<u8>,
    candidate_actor_bytes: Vec<u8>,
    candidate_provenance_bytes: Vec<u8>,
    change: ProofImpactChange,
    verdict: ProofImpactVerdict,
}

impl Fixture {
    fn new() -> Self {
        let id = FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("peritus-candidate-actors-{}-{id}", std::process::id()));
        fs::create_dir_all(root.join("verification")).expect("fixture directory");
        command(&root, &["init", "--quiet"]);
        command(&root, &["config", "user.name", "Peritus Test"]);
        command(&root, &["config", "user.email", "peritus-test@example.invalid"]);
        command(&root, &["config", "commit.gpgsign", "false"]);

        let base_provenance_bytes = provenance_document(BASE);
        let base_actor_bytes = actor_document(BASE, &sha256_hex(&base_provenance_bytes));
        write(&root, ACTORS_PATH, &base_actor_bytes);
        write(&root, PROVENANCE_PATH, &base_provenance_bytes);
        command(&root, &["add", ACTORS_PATH, PROVENANCE_PATH]);
        command(&root, &["commit", "--quiet", "-m", "protected"]);

        let all: Vec<_> = BASE.iter().chain(FRESH).copied().collect();
        let candidate_provenance_bytes = provenance_document(&all);
        let candidate_actor_bytes = actor_document(&all, &sha256_hex(&candidate_provenance_bytes));
        let protected =
            toml::from_str(std::str::from_utf8(&base_actor_bytes).expect("protected actor UTF-8"))
                .expect("protected actor document");
        let mut fixture = Self {
            root,
            protected,
            base_actor_bytes,
            base_provenance_bytes,
            candidate_actor_bytes,
            candidate_provenance_bytes,
            change: change(),
            verdict: verdict(),
        };
        fixture.install_candidate();
        fixture
    }

    fn install_candidate(&mut self) {
        write(&self.root, ACTORS_PATH, &self.candidate_actor_bytes);
        write(&self.root, PROVENANCE_PATH, &self.candidate_provenance_bytes);
        command(&self.root, &["add", ACTORS_PATH, PROVENANCE_PATH]);
        command(&self.root, &["commit", "--quiet", "-m", "candidate"]);
        self.verdict.implementation_tree = stdout(&self.root, &["rev-parse", "HEAD^{tree}"]);
        self.change.source_changes = vec![
            transition(ACTORS_PATH, &self.base_actor_bytes, &self.candidate_actor_bytes),
            transition(
                PROVENANCE_PATH,
                &self.base_provenance_bytes,
                &self.candidate_provenance_bytes,
            ),
        ];
        write(&self.root, ACTORS_PATH, &self.base_actor_bytes);
        write(&self.root, PROVENANCE_PATH, &self.base_provenance_bytes);
    }

    fn retain_owner_and_append_reviewer(&mut self) {
        let specs: Vec<_> = BASE.iter().chain(&FRESH[1..]).copied().collect();
        self.candidate_provenance_bytes = provenance_document(&specs);
        self.candidate_actor_bytes =
            actor_document(&specs, &sha256_hex(&self.candidate_provenance_bytes));
        self.change.owner = "ACTOR-0001".to_owned();
        self.install_candidate();
    }

    fn diagnostics(&self) -> (Option<CandidateActors>, Vec<Diagnostic>) {
        let mut diagnostics = Vec::new();
        let candidate = CandidateActors::load(
            &self.root,
            &self.protected,
            &self.change,
            &self.verdict,
            &mut diagnostics,
        );
        (candidate, diagnostics)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn exact_candidate_append_builds_the_reusable_registry() {
    let fixture = Fixture::new();
    let (candidate, mut diagnostics) = fixture.diagnostics();
    let candidate = candidate.expect("valid candidate must load");
    let registry = candidate.registry(&mut diagnostics);
    registry.validate_pair(
        Path::new(MANIFEST),
        &fixture.change.id,
        &fixture.change.owner,
        &fixture.change.reviewer,
        &mut diagnostics,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn registered_owner_is_preserved_while_fresh_reviewer_is_appended() {
    let mut fixture = Fixture::new();
    fixture.retain_owner_and_append_reviewer();
    let (candidate, mut diagnostics) = fixture.diagnostics();
    let candidate = candidate.expect("valid candidate must load");
    let registry = candidate.registry(&mut diagnostics);
    registry.validate_pair(
        Path::new(MANIFEST),
        &fixture.change.id,
        &fixture.change.owner,
        &fixture.change.reviewer,
        &mut diagnostics,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn protected_reviewer_cannot_be_reused_for_a_new_candidate() {
    let mut fixture = Fixture::new();
    fixture.change.reviewer = "ACTOR-0002".to_owned();
    fixture.verdict.reviewer = fixture.change.reviewer.clone();
    fixture.verdict.reviewer_principal = principal(BASE[1]);
    let (_, diagnostics) = fixture.diagnostics();
    assert!(diagnostics.iter().any(|item| item.message().contains("is not fresh")));
}

#[test]
fn historical_identity_rewrite_and_missing_pcr_binding_fail_closed() {
    let mut fixture = Fixture::new();
    fixture.candidate_actor_bytes = String::from_utf8(fixture.candidate_actor_bytes.clone())
        .expect("candidate UTF-8")
        .replace("Historical Owner", "Rewritten Owner")
        .into_bytes();
    fixture.install_candidate();
    fixture.change.source_changes.pop();
    let (_, diagnostics) = fixture.diagnostics();
    assert!(diagnostics.iter().any(|item| item.message().contains("rewrites protected actor")));
    assert!(diagnostics.iter().any(|item| item.message().contains("does not exactly bind")));
}

#[test]
fn unknown_candidate_field_and_reviewer_principal_substitution_are_rejected() {
    let mut malformed = Fixture::new();
    malformed.candidate_actor_bytes.extend_from_slice(b"unknown = true\n");
    malformed.install_candidate();
    let (candidate, diagnostics) = malformed.diagnostics();
    assert!(candidate.is_none());
    assert!(diagnostics.iter().any(|item| item.message().contains("TOML schema")));

    let mut substituted = Fixture::new();
    substituted.verdict.reviewer_principal = "substituted-principal".to_owned();
    let (_, diagnostics) = substituted.diagnostics();
    assert!(diagnostics.iter().any(|item| item.message().contains("verdict principal")));
}

fn actor_document(specs: &[ActorSpec], provenance_sha256: &str) -> Vec<u8> {
    let mut text = String::from(
        "schema = \"peritus.verification.actors\"\nschema_version = 1\nbaseline = \"A1\"\n",
    );
    for spec in specs {
        let principal = principal(*spec);
        write!(
            text,
            "\n[[entries]]\nid = \"{}\"\nkind = \"codex-subagent\"\nprincipal = \"{principal}\"\ndisplay_name = \"{}\"\nroles = [\"{}\"]\n\n[entries.provenance]\nrecord_path = \"{PROVENANCE_PATH}\"\nrecord_sha256 = \"{provenance_sha256}\"\n",
            spec.id, spec.name, spec.role,
        )
        .expect("format actor document");
    }
    text.into_bytes()
}

fn provenance_document(specs: &[ActorSpec]) -> Vec<u8> {
    let entries: Vec<_> = specs
        .iter()
        .map(|spec| {
            let principal = principal(*spec);
            let reviewer = spec.role == "reviewer";
            json!({
                "actor_id": spec.id,
                "kind": "codex-subagent",
                "principal": principal,
                "repository": REPOSITORY,
                "issue": 45,
                "issue_created_at": "2026-08-22T22:17:03Z",
                "session": spec.session,
                "task": spec.task,
                "mode": if reviewer { "read-only-review" } else { "implementation" },
                "model": if reviewer { Some("gpt-5.6-sol") } else { None },
                "reasoning_effort": if reviewer { Some("xhigh") } else { None },
                "public_key": null,
                "allowed_signer": null,
                "record_locators": [format!("codex-collaboration:{principal}")],
            })
        })
        .collect();
    let mut bytes = serde_json::to_vec_pretty(&json!({
        "schema": "peritus.verification.actor-provenance",
        "schema_version": 1,
        "baseline": "A1",
        "entries": entries,
    }))
    .expect("serialize provenance");
    bytes.push(b'\n');
    bytes
}

fn principal(spec: ActorSpec) -> String {
    format!("{REPOSITORY}/session/{}/task{}", spec.session, spec.task)
}

fn change() -> ProofImpactChange {
    ProofImpactChange {
        id: "PCR-0005".to_owned(),
        status: ProofImpactStatus::Approved,
        change_kinds: Vec::new(),
        source_changes: Vec::new(),
        rationale: "candidate actor migration".to_owned(),
        impact: "fresh independently reviewed identities".to_owned(),
        evidence: Vec::new(),
        owner: "ACTOR-0003".to_owned(),
        reviewer: "ACTOR-0004".to_owned(),
        review_date: "2026-09-09".to_owned(),
        verdict: None,
    }
}

fn verdict() -> ProofImpactVerdict {
    let empty = ProofImpactVerdictArtifactRef { path: String::new(), sha256: String::new() };
    ProofImpactVerdict {
        schema: String::new(),
        schema_version: 1,
        id: String::new(),
        pcr_id: "PCR-0005".to_owned(),
        reviewer: "ACTOR-0004".to_owned(),
        reviewer_principal: principal(FRESH[1]),
        authorization_base_commit: "1".repeat(40),
        implementation_commit: "2".repeat(40),
        implementation_tree: String::new(),
        source_transitions_sha256: String::new(),
        gate_evidence_sha256: String::new(),
        finding_set_sha256: String::new(),
        artifact_inventory_sha256: String::new(),
        decision: ProofImpactVerdictDecision::Approved,
        reviewed_at: String::new(),
        review_report: empty,
        gate_evidence: Vec::new(),
        findings: Vec::new(),
        artifacts: Vec::new(),
    }
}

fn transition(path: &str, previous: &[u8], current: &[u8]) -> ProofSourceChange {
    let package =
        || ProofImpactPackage { package: "xtask".to_owned(), verification_class: "T".to_owned() };
    ProofSourceChange {
        source_file: path.to_owned(),
        previous: Some(ProofImpactSnapshot {
            sha256: sha256_hex(previous),
            affected_packages: vec![package()],
        }),
        current: Some(ProofImpactSnapshot {
            sha256: sha256_hex(current),
            affected_packages: vec![package()],
        }),
    }
}

fn write(root: &Path, relative: &str, bytes: &[u8]) {
    fs::write(root.join(relative), bytes).expect("write fixture file");
}

fn command(root: &Path, arguments: &[&str]) -> Output {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .expect("Git fixture command");
    assert!(output.status.success(), "Git failed: {}", String::from_utf8_lossy(&output.stderr));
    output
}

fn stdout(root: &Path, arguments: &[&str]) -> String {
    String::from_utf8(command(root, arguments).stdout).expect("Git output UTF-8").trim().to_owned()
}
