//! Black-box three-host H0 aggregation and final evidence publication.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use peritus_security_qualification::{
    CancellationToken, CleanupObservation, EvidenceEntry, EvidenceSet, EvidenceValue,
    FreshSubjectFactory, IntegratedCandidate, NativeExecutionReceipt, ProbeObservation,
    ProbeOutcome, ProbeRequest, ProbeSpec, QualificationError, QualificationLimits,
    QualificationPlatform, QualificationRunner, QualificationSubject, ResourceUsage,
    SafeEvidenceCode, candidate_json, digest_bytes, hex_digest,
};
use peritus_types::{
    AcceptanceSpecId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, Sha256Digest, WorkspaceId,
};
use serde_json::{Value, json};

#[test]
fn aggregate_operator_publishes_one_ready_candidate_bound_report() {
    let root = tempfile::tempdir().expect("aggregate root");
    let candidate = candidate(1);
    let paths = write_shards(root.path(), candidate);
    let review = root.path().join("review.json");
    fs::write(
        &review,
        serde_json::to_vec_pretty(&review_document(candidate, "completed")).expect("review JSON"),
    )
    .expect("write review");
    let report = root.path().join("final-report.json");

    let first = run_operator(&paths, &review, &report);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    let document: Value =
        serde_json::from_slice(&fs::read(&report).expect("final report")).expect("report JSON");
    assert_eq!(document["status"], "ready");
    assert_eq!(document["probe_count"], 42);
    assert_eq!(document["not_ready_reasons"], json!([]));
    let manifest = document["evidence_manifest_json"].as_str().expect("manifest JSON string");
    let _: Value = serde_json::from_str(manifest).expect("embedded manifest JSON");
    assert_eq!(document["evidence_manifest_sha256"], hex_digest(digest_bytes(manifest.as_bytes())));

    let second = run_operator(&paths, &review, &report);
    assert!(!second.status.success());
    assert!(String::from_utf8_lossy(&second.stderr).contains("report path already exists"));

    let incomplete_review = root.path().join("incomplete-review.json");
    fs::write(
        &incomplete_review,
        serde_json::to_vec_pretty(&review_document(candidate, "incomplete"))
            .expect("incomplete review JSON"),
    )
    .expect("write incomplete review");
    let not_ready_report = root.path().join("not-ready-report.json");
    let not_ready = run_operator(&paths, &incomplete_review, &not_ready_report);
    assert!(!not_ready.status.success());
    let not_ready_document: Value =
        serde_json::from_slice(&fs::read(&not_ready_report).expect("not-ready report"))
            .expect("not-ready report JSON");
    assert_eq!(not_ready_document["status"], "not-ready");
    assert!(not_ready_document["not_ready_reasons"].as_array().is_some_and(|reasons| {
        reasons.iter().any(|reason| reason["code"] == "external-review-incomplete")
    }));
}

struct ShardPaths {
    linux: PathBuf,
    macos: PathBuf,
    windows: PathBuf,
}

fn write_shards(parent: &Path, candidate: IntegratedCandidate) -> ShardPaths {
    let limits = QualificationLimits::production();
    let cancellation = CancellationToken::new();
    let mut paths = Vec::new();
    for platform in QualificationPlatform::ALL {
        let shard = QualificationRunner
            .run_shard(
                &mut PassingFactory::new(platform),
                candidate,
                limits,
                &cancellation,
                platform,
            )
            .expect("passing shard");
        let path = parent.join(format!("{}.json", platform.as_str()));
        fs::write(&path, shard.canonical_json().expect("shard JSON")).expect("write shard");
        paths.push(path);
    }
    ShardPaths { linux: paths.remove(0), macos: paths.remove(0), windows: paths.remove(0) }
}

fn run_operator(paths: &ShardPaths, review: &Path, report: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_peritus-h0-aggregate"))
        .args(["--linux", paths.linux.to_str().expect("Linux path")])
        .args(["--macos", paths.macos.to_str().expect("macOS path")])
        .args(["--windows", paths.windows.to_str().expect("Windows path")])
        .args(["--review", review.to_str().expect("review path")])
        .args(["--report", report.to_str().expect("report path")])
        .output()
        .expect("run aggregate operator")
}

fn review_document(candidate: IntegratedCandidate, completion: &str) -> Value {
    let envelope: Value =
        serde_json::from_slice(&candidate_json(candidate).expect("candidate JSON"))
            .expect("candidate envelope");
    json!({
        "candidate": envelope.get("candidate").expect("candidate object").clone(),
        "reviewer_actor": repeated_hex(20, 16),
        "reviewer_organization_sha256": repeated_hex(20, 32),
        "review_context_sha256": repeated_hex(21, 32),
        "producer_actor": repeated_hex(10, 16),
        "producer_organization_sha256": repeated_hex(10, 32),
        "completion": completion,
        "scopes": [
            "sandbox-escape",
            "authority-isolation",
            "evolution-and-promotion",
            "supply-chain",
            "unsafe-and-tcb"
        ],
        "independent_from_producer": true,
        "report_sha256": repeated_hex(22, 32),
        "findings": []
    })
}

struct PassingFactory {
    platform: QualificationPlatform,
    next: u32,
}

impl PassingFactory {
    const fn new(platform: QualificationPlatform) -> Self {
        Self { platform, next: 0 }
    }
}

impl FreshSubjectFactory for PassingFactory {
    fn create(
        &mut self,
        candidate: IntegratedCandidate,
        spec: ProbeSpec,
        _limits: QualificationLimits,
        _cancellation: &CancellationToken,
    ) -> Result<Box<dyn QualificationSubject>, QualificationError> {
        self.next += 1;
        Ok(Box::new(PassingSubject {
            id: format!("{}-{}", self.platform.as_str(), self.next),
            candidate,
            spec,
        }))
    }
}

struct PassingSubject {
    id: String,
    candidate: IntegratedCandidate,
    spec: ProbeSpec,
}

impl QualificationSubject for PassingSubject {
    fn subject_id(&self) -> &str {
        &self.id
    }

    fn execute(
        &mut self,
        _request: ProbeRequest<'_>,
    ) -> Result<ProbeObservation, QualificationError> {
        let mut evidence = EvidenceSet::new();
        evidence.insert(EvidenceEntry::new(
            SafeEvidenceCode::new("assertion.observed")?,
            EvidenceValue::Fact(true),
        ))?;
        let receipt = NativeExecutionReceipt::from_native_observation(
            digest(40),
            digest(41),
            digest(42),
            0,
            true,
            ResourceUsage::new(1, 1, 1, 1, 0),
            evidence,
        )?;
        Ok(ProbeObservation::from_native_execution(
            self.candidate,
            self.spec.id(),
            ProbeOutcome::Passed,
            receipt,
        ))
    }

    fn cleanup(self: Box<Self>) -> Result<CleanupObservation, QualificationError> {
        CleanupObservation::new(self.id, 0, 0, 0, 0, digest(43))
    }
}

fn candidate(seed: u8) -> IntegratedCandidate {
    IntegratedCandidate::new(
        RevisionTuple::new(
            AcceptanceSpecId::new([seed; 16]).expect("acceptance"),
            HarnessId::new([seed.wrapping_add(1); 16]).expect("harness"),
            WorkspaceId::new([seed.wrapping_add(2); 16]).expect("workspace"),
            Generation::first(),
            RevisionNumber::first(),
            PolicyId::new([seed.wrapping_add(3); 16]).expect("policy"),
            ProviderProfileId::new([seed.wrapping_add(4); 16]).expect("provider"),
        ),
        digest(seed),
        digest(seed.wrapping_add(10)),
        digest(seed.wrapping_add(20)),
    )
}

fn repeated_hex(byte: u8, bytes: usize) -> String {
    format!("{byte:02x}").repeat(bytes)
}

const fn digest(seed: u8) -> Sha256Digest {
    Sha256Digest::new([seed; 32])
}
