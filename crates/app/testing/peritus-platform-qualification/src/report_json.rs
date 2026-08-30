//! Stable retained JSON projection for a completed H2 qualification report.

use serde::Serialize;

use crate::{
    EvidenceKind, NotReadyReason, QualificationReport, ReadinessVerdict, ScenarioObservation,
};

#[derive(Serialize)]
struct ReportDocument<'a> {
    schema_version: u8,
    target: TargetDocument,
    manifest_sha256: String,
    verdict: VerdictDocument,
    scenarios: Vec<ScenarioDocument<'a>>,
}

#[derive(Serialize)]
struct TargetDocument {
    platform: &'static str,
    architecture: &'static str,
    version: VersionDocument,
}

#[derive(Serialize)]
struct VersionDocument {
    major: u16,
    minor: u16,
    patch: u16,
    build: u32,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
enum VerdictDocument {
    Ready { scenario_count: usize, evidence_sha256: String },
    NotReady { reasons: Vec<ReasonDocument> },
}

#[derive(Serialize)]
struct ReasonDocument {
    kind: &'static str,
    scenario_id: &'static str,
}

#[derive(Serialize)]
struct ScenarioDocument<'a> {
    scenario_id: &'static str,
    subject_id: &'a str,
    outcome: &'static str,
    evidence_sha256: String,
    evidence: Vec<EvidenceDocument<'a>>,
    cleanup: CleanupDocument<'a>,
}

#[derive(Serialize)]
struct CleanupDocument<'a> {
    subject_id: &'a str,
    complete: bool,
    remaining_resources: u32,
    evidence_sha256: String,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum EvidenceDocument<'a> {
    Text { label: &'a str, value: &'a str },
    Digest { label: &'a str, sha256: String, bytes: u64 },
    Count { label: &'a str, value: u64 },
    Fact { label: &'a str, value: bool },
}

pub fn render(report: &QualificationReport) -> Result<Vec<u8>, serde_json::Error> {
    let run = report.run();
    let target = run.target();
    let version = target.version();
    let document = ReportDocument {
        schema_version: 1,
        target: TargetDocument {
            platform: target.platform().as_str(),
            architecture: target.architecture().as_str(),
            version: VersionDocument {
                major: version.major(),
                minor: version.minor(),
                patch: version.patch(),
                build: version.build(),
            },
        },
        manifest_sha256: run.manifest_digest().to_hex(),
        verdict: verdict(report.verdict()),
        scenarios: run.observations().iter().map(scenario).collect(),
    };
    let mut bytes = serde_json::to_vec_pretty(&document)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn verdict(verdict: &ReadinessVerdict) -> VerdictDocument {
    match verdict {
        ReadinessVerdict::Ready(evidence) => VerdictDocument::Ready {
            scenario_count: evidence.scenario_count(),
            evidence_sha256: evidence.evidence_digest().to_hex(),
        },
        ReadinessVerdict::NotReady(reasons) => {
            VerdictDocument::NotReady { reasons: reasons.iter().copied().map(reason).collect() }
        }
    }
}

const fn reason(reason: NotReadyReason) -> ReasonDocument {
    match reason {
        NotReadyReason::ScenarioFailed(scenario) => {
            ReasonDocument { kind: "scenario-failed", scenario_id: scenario.as_str() }
        }
        NotReadyReason::ScenarioUnsupported(scenario) => {
            ReasonDocument { kind: "scenario-unsupported", scenario_id: scenario.as_str() }
        }
        NotReadyReason::CleanupIncomplete(scenario) => {
            ReasonDocument { kind: "cleanup-incomplete", scenario_id: scenario.as_str() }
        }
    }
}

fn scenario(observation: &ScenarioObservation) -> ScenarioDocument<'_> {
    let cleanup =
        observation.cleanup().expect("QualificationRun guarantees cleanup for every scenario");
    ScenarioDocument {
        scenario_id: observation.scenario().as_str(),
        subject_id: observation.subject_id(),
        outcome: match observation.outcome() {
            crate::ObservationOutcome::Passed => "passed",
            crate::ObservationOutcome::Failed => "failed",
            crate::ObservationOutcome::Unsupported => "unsupported",
        },
        evidence_sha256: observation.evidence().digest().to_hex(),
        evidence: observation
            .evidence()
            .entries()
            .iter()
            .map(|entry| match entry.kind() {
                EvidenceKind::Text(value) => {
                    EvidenceDocument::Text { label: entry.label(), value: value.as_str() }
                }
                EvidenceKind::Digest { sha256, bytes } => EvidenceDocument::Digest {
                    label: entry.label(),
                    sha256: sha256.to_hex(),
                    bytes: *bytes,
                },
                EvidenceKind::Count(value) => {
                    EvidenceDocument::Count { label: entry.label(), value: *value }
                }
                EvidenceKind::Fact(value) => {
                    EvidenceDocument::Fact { label: entry.label(), value: *value }
                }
            })
            .collect(),
        cleanup: CleanupDocument {
            subject_id: cleanup.subject_id(),
            complete: cleanup.complete(),
            remaining_resources: cleanup.remaining_resources(),
            evidence_sha256: cleanup.digest().to_hex(),
        },
    }
}
