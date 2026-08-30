//! Strict request decoding and binding for the persistent H1 controller.

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::args::{ControllerPaths, lower_sha256};

pub(super) const SUPPORTED_SCENARIO: &str = "h1.crash.journal.after-before-ack";
const MAX_REQUEST_BYTES: usize = 512 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema_version: u8,
    request_sha256: String,
    request: RequestDocument,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RequestDocument {
    schema_version: u8,
    stage: String,
    sequence: u8,
    instance_id: String,
    subject: SubjectDocument,
    executor_sha256: String,
    scenario: ScenarioDocument,
    limits: LimitsDocument,
    artifact_contract: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SubjectDocument {
    id: String,
    implementation: String,
    build_sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ScenarioDocument {
    id: String,
    title: String,
    fault: FaultDocument,
    expected_recovery: String,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum FaultDocument {
    CommitCrash { boundary: String, timing: String },
    Corruption { target: String },
    DiskExhaustion { scope: String },
    DependencyDeath { dependency: String },
    RetryExhaustion { dependency: String },
    DaemonKill { phase: String },
    HostReboot { phase: String },
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LimitsDocument {
    max_scenarios: u16,
    max_milestones: u16,
    provider_retries: u16,
    tool_retries: u16,
    worker_restarts: u16,
    reconciliation_steps: u16,
    events: u32,
    evidence_bytes: u32,
    owned_processes: u16,
    cleanup_steps: u16,
    logical_ticks: u64,
}

pub(super) struct BoundRequest {
    pub(super) document: RequestDocument,
    pub(super) request_sha256: String,
}

impl BoundRequest {
    pub(super) fn decode(
        line: &[u8],
        paths: &ControllerPaths,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        if line.is_empty() || line.len() > MAX_REQUEST_BYTES || line.contains(&0) {
            return Err("H1 controller request is empty, oversized, or contains NUL".into());
        }
        let envelope: Envelope = serde_json::from_slice(line)?;
        if envelope.schema_version != 1 || !lower_sha256(&envelope.request_sha256) {
            return Err("H1 controller envelope schema or digest is invalid".into());
        }
        let canonical = serde_json::to_vec(&envelope.request)?;
        let observed = hex(&Sha256::digest(canonical));
        if observed != envelope.request_sha256 {
            return Err("H1 request digest differs from the canonical request".into());
        }
        let bound = Self { document: envelope.request, request_sha256: envelope.request_sha256 };
        bound.validate(paths)?;
        Ok(bound)
    }

    fn validate(&self, paths: &ControllerPaths) -> Result<(), Box<dyn std::error::Error>> {
        let request = &self.document;
        if request.schema_version != 1
            || request.instance_id != paths.instance_id
            || request.subject.id != paths.subject_id
            || request.subject.build_sha256 != paths.build_sha256
            || request.executor_sha256 != paths.executor_sha256
            || request.artifact_contract != "peritus-h1-retained-artifacts-v1"
            || request.subject.implementation.is_empty()
            || request.subject.implementation.len() > 1_024
        {
            return Err("H1 request identity differs from the staged controller invocation".into());
        }
        if request.scenario.id != SUPPORTED_SCENARIO
            || request.scenario.title.is_empty()
            || request.scenario.title.len() > 1_024
            || request.scenario.expected_recovery != "replayed-committed"
            || !matches!(
                &request.scenario.fault,
                FaultDocument::CommitCrash { boundary, timing }
                    if boundary == "journal"
                        && timing == "after-durable-commit-before-ack"
            )
        {
            return Err(
                "production H1 controller has no genuine effect route for this scenario".into()
            );
        }
        validate_limits(&request.limits)
    }

    pub(super) fn stage(&self) -> Result<Stage, Box<dyn std::error::Error>> {
        Stage::parse(&self.document.stage)
    }

    pub(super) const fn sequence(&self) -> u8 {
        self.document.sequence
    }

    pub(super) fn scenario_id(&self) -> &str {
        &self.document.scenario.id
    }

    pub(super) const fn limits(&self) -> &LimitsDocument {
        &self.document.limits
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Stage {
    Prepare,
    Inject,
    Recover,
    Cleanup,
}

impl Stage {
    fn parse(value: &str) -> Result<Self, Box<dyn std::error::Error>> {
        match value {
            "prepare" => Ok(Self::Prepare),
            "inject" => Ok(Self::Inject),
            "recover" => Ok(Self::Recover),
            "cleanup" => Ok(Self::Cleanup),
            _ => Err("H1 controller request contains an unknown stage".into()),
        }
    }

    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::Prepare => "prepare",
            Self::Inject => "inject",
            Self::Recover => "recover",
            Self::Cleanup => "cleanup",
        }
    }
}

impl LimitsDocument {
    pub(super) const fn evidence_bytes(&self) -> u32 {
        self.evidence_bytes
    }

    pub(super) const fn cleanup_steps(&self) -> u16 {
        self.cleanup_steps
    }

    pub(super) const fn logical_ticks(&self) -> u64 {
        self.logical_ticks
    }
}

fn validate_limits(limits: &LimitsDocument) -> Result<(), Box<dyn std::error::Error>> {
    if !(1..=128).contains(&limits.max_scenarios)
        || !(6..=64).contains(&limits.max_milestones)
        || !(1..=32).contains(&limits.provider_retries)
        || !(1..=32).contains(&limits.tool_retries)
        || !(1..=32).contains(&limits.worker_restarts)
        || !(1..=32).contains(&limits.reconciliation_steps)
        || limits.events == 0
        || limits.evidence_bytes == 0
        || limits.owned_processes == 0
        || limits.cleanup_steps == 0
        || limits.logical_ticks == 0
    {
        Err("H1 controller request contains invalid resource limits".into())
    } else {
        Ok(())
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}
