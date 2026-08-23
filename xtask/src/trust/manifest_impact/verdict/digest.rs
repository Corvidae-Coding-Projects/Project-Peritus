use crate::trust::manifest_model::{
    ProofImpactChange, ProofImpactEvidenceKind, ProofImpactFindingDisposition,
    ProofImpactFindingSeverity, ProofImpactGateResult, ProofImpactSnapshot,
    ProofImpactVerdictArtifact, ProofImpactVerdictArtifactKind, ProofImpactVerdictFinding,
    ProofImpactVerdictGate,
};
use sha2::{Digest, Sha256};

pub(super) fn source_transitions(change: &ProofImpactChange) -> String {
    let mut sources: Vec<_> = change.source_changes.iter().collect();
    sources.sort_by_key(|source| source.source_file.as_str());
    let mut hash = CanonicalHash::new(b"peritus-proof-source-transitions-v1");
    for source in sources {
        hash.field(source.source_file.as_bytes());
        hash.snapshot(source.previous.as_ref());
        hash.snapshot(source.current.as_ref());
    }
    hash.finish()
}

pub(super) fn gate_evidence(gates: &[ProofImpactVerdictGate]) -> String {
    let mut gates: Vec<_> = gates.iter().collect();
    gates.sort_by_key(|gate| (gate.kind, gate.owning_crate.as_str(), gate.command.as_str()));
    let mut hash = CanonicalHash::new(b"peritus-proof-gate-evidence-v1");
    for gate in gates {
        hash.field(evidence_kind(gate.kind).as_bytes());
        hash.field(gate.owning_crate.as_bytes());
        hash.field(gate.command.as_bytes());
        hash.field(gate_result(gate.result).as_bytes());
        hash.field(gate.output.path.as_bytes());
        hash.field(gate.output.sha256.as_bytes());
    }
    hash.finish()
}

pub(super) fn findings(findings: &[ProofImpactVerdictFinding]) -> String {
    let mut findings: Vec<_> = findings.iter().collect();
    findings.sort_by_key(|finding| finding.id.as_str());
    let mut hash = CanonicalHash::new(b"peritus-proof-findings-v1");
    for finding in findings {
        hash.field(finding.id.as_bytes());
        hash.field(severity(finding.severity).as_bytes());
        hash.field(if finding.blocking { b"true" } else { b"false" });
        hash.field(disposition(finding.disposition).as_bytes());
        hash.field(finding.detail.path.as_bytes());
        hash.field(finding.detail.sha256.as_bytes());
        hash.field(finding.evidence.path.as_bytes());
        hash.field(finding.evidence.sha256.as_bytes());
    }
    hash.finish()
}

pub(super) fn artifact_inventory(artifacts: &[ProofImpactVerdictArtifact]) -> String {
    let mut artifacts: Vec<_> = artifacts.iter().collect();
    artifacts.sort();
    let mut hash = CanonicalHash::new(b"peritus-proof-review-artifacts-v1");
    for artifact in artifacts {
        hash.field(artifact_kind(artifact.kind).as_bytes());
        hash.field(artifact.path.as_bytes());
        hash.field(artifact.sha256.as_bytes());
    }
    hash.finish()
}

const fn evidence_kind(kind: ProofImpactEvidenceKind) -> &'static str {
    match kind {
        ProofImpactEvidenceKind::OrdinaryTest => "ordinary-test",
        ProofImpactEvidenceKind::VerusVerify => "verus-verify",
    }
}

const fn gate_result(result: ProofImpactGateResult) -> &'static str {
    match result {
        ProofImpactGateResult::Passed => "passed",
        ProofImpactGateResult::Failed => "failed",
    }
}

const fn severity(value: ProofImpactFindingSeverity) -> &'static str {
    match value {
        ProofImpactFindingSeverity::Critical => "critical",
        ProofImpactFindingSeverity::High => "high",
        ProofImpactFindingSeverity::Medium => "medium",
        ProofImpactFindingSeverity::Low => "low",
        ProofImpactFindingSeverity::Informational => "informational",
    }
}

const fn disposition(value: ProofImpactFindingDisposition) -> &'static str {
    match value {
        ProofImpactFindingDisposition::Open => "open",
        ProofImpactFindingDisposition::Fixed => "fixed",
        ProofImpactFindingDisposition::Invalid => "invalid",
        ProofImpactFindingDisposition::Superseded => "superseded",
    }
}

const fn artifact_kind(value: ProofImpactVerdictArtifactKind) -> &'static str {
    match value {
        ProofImpactVerdictArtifactKind::ReviewReport => "review-report",
        ProofImpactVerdictArtifactKind::GateOutput => "gate-output",
        ProofImpactVerdictArtifactKind::FindingDetail => "finding-detail",
        ProofImpactVerdictArtifactKind::FindingEvidence => "finding-evidence",
    }
}

struct CanonicalHash(Sha256);

impl CanonicalHash {
    fn new(domain: &[u8]) -> Self {
        let mut hash = Sha256::new();
        hash.update(domain);
        Self(hash)
    }

    fn field(&mut self, value: &[u8]) {
        self.0.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
        self.0.update(value);
    }

    fn snapshot(&mut self, snapshot: Option<&ProofImpactSnapshot>) {
        let Some(snapshot) = snapshot else {
            self.field(b"absent");
            return;
        };
        self.field(b"present");
        self.field(snapshot.sha256.as_bytes());
        for package in &snapshot.affected_packages {
            self.field(package.package.as_bytes());
            self.field(package.verification_class.as_bytes());
        }
    }

    fn finish(self) -> String {
        self.0.finalize().iter().fold(String::with_capacity(64), |mut output, byte| {
            use std::fmt::Write as _;
            write!(output, "{byte:02x}").expect("writing to a String cannot fail");
            output
        })
    }
}
