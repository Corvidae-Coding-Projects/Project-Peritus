//! Canonical retained evidence and bound response publication.

use std::fs;

use serde::Serialize;

use crate::digest_bytes;

use super::args::ControllerPaths;
use super::checks::{CheckOutcome, Observation};
use super::request::BoundRequest;

#[derive(Serialize)]
struct ScenarioArtifact<'a> {
    schema_version: u8,
    subject_id: &'a str,
    scenario_id: &'a str,
    request_sha256: &'a str,
    observation: &'a Observation,
}

#[derive(Serialize)]
struct ResponseDocument<'a> {
    schema_version: u8,
    subject_id: &'a str,
    scenario_id: &'a str,
    request_sha256: &'a str,
    outcome: &'static str,
    artifact_count: u32,
    evidence: Vec<EvidenceDocument<'a>>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum EvidenceDocument<'a> {
    Fact { label: &'a str, value: bool },
    Count { label: &'a str, value: u64 },
    Text { label: &'a str, value: &'a str },
    Digest { label: &'a str, path: &'a str, sha256: String, bytes: u64 },
}

#[derive(Serialize)]
struct CleanupDocument<'a> {
    schema_version: u8,
    subject_id: &'a str,
    scenario_id: &'a str,
    request_sha256: &'a str,
    complete: bool,
    remaining_resources: u32,
    evidence: ArtifactDocument,
}

#[derive(Serialize)]
struct ArtifactDocument {
    path: &'static str,
    sha256: String,
    bytes: u64,
}

pub(super) fn publish(
    paths: &ControllerPaths,
    request: &BoundRequest,
    observation: &Observation,
) -> Result<(), Box<dyn std::error::Error>> {
    let scenario_path = "scenario-observation.json";
    let scenario_bytes = serde_json::to_vec_pretty(&ScenarioArtifact {
        schema_version: 1,
        subject_id: &paths.subject_id,
        scenario_id: request.scenario_id(),
        request_sha256: &paths.request_sha256,
        observation,
    })?;
    require_artifact_bound(&scenario_bytes, request)?;
    fs::write(paths.artifact_root.join(scenario_path), &scenario_bytes)?;
    let scenario_digest = digest_bytes(&scenario_bytes);
    let mut evidence = Vec::with_capacity(observation.facts.len() + observation.counts.len() + 3);
    for fact in &observation.facts {
        evidence.push(EvidenceDocument::Fact { label: &fact.label, value: fact.value });
    }
    for count in &observation.counts {
        evidence.push(EvidenceDocument::Count { label: &count.label, value: count.value });
    }
    evidence.push(EvidenceDocument::Text { label: "native.summary", value: &observation.summary });
    evidence.push(EvidenceDocument::Text {
        label: "native.platform",
        value: request.document.target().platform_name(),
    });
    evidence.push(EvidenceDocument::Digest {
        label: "native.raw-observation",
        path: scenario_path,
        sha256: scenario_digest.sha256().to_hex(),
        bytes: scenario_digest.byte_length(),
    });
    let response = ResponseDocument {
        schema_version: 1,
        subject_id: &paths.subject_id,
        scenario_id: request.scenario_id(),
        request_sha256: &paths.request_sha256,
        outcome: observation.outcome.as_str(),
        artifact_count: 1,
        evidence,
    };
    publish_new(&paths.response, &serde_json::to_vec(&response)?)?;

    let cleanup_path = "cleanup-observation.json";
    let cleanup_bytes = format!(
        "peritus-h2-native-cleanup-v1\nsubject={}\nscenario={}\nremaining_resources=0\n",
        paths.subject_id,
        request.scenario_id()
    )
    .into_bytes();
    require_artifact_bound(&cleanup_bytes, request)?;
    fs::write(paths.artifact_root.join(cleanup_path), &cleanup_bytes)?;
    let cleanup_digest = digest_bytes(&cleanup_bytes);
    let cleanup = CleanupDocument {
        schema_version: 1,
        subject_id: &paths.subject_id,
        scenario_id: request.scenario_id(),
        request_sha256: &paths.request_sha256,
        complete: true,
        remaining_resources: 0,
        evidence: ArtifactDocument {
            path: cleanup_path,
            sha256: cleanup_digest.sha256().to_hex(),
            bytes: cleanup_digest.byte_length(),
        },
    };
    publish_new(&paths.cleanup_response, &serde_json::to_vec(&cleanup)?)?;
    Ok(())
}

fn require_artifact_bound(
    bytes: &[u8],
    request: &BoundRequest,
) -> Result<(), Box<dyn std::error::Error>> {
    if bytes.is_empty() || bytes.len() as u64 > request.document.limits().artifact_bytes() {
        Err("H2 controller artifact exceeds its request bound".into())
    } else {
        Ok(())
    }
}

fn publish_new(path: &std::path::Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write as _;

    let parent = path.parent().ok_or("H2 response path has no parent")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(path)?;
    Ok(())
}

impl CheckOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Unsupported => "unsupported",
        }
    }
}
