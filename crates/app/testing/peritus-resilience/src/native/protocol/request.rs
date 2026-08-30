//! Canonical native H1 controller requests.

use serde::Serialize;

use crate::{
    FaultInjection, QualificationConfig, ScenarioSpec, SubjectDescriptor, SubjectErrorCode,
};

use super::super::{digest, subject_error};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::native) enum Stage {
    Prepare,
    Inject,
    Recover,
    Cleanup,
}

impl Stage {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::Prepare => "prepare",
            Self::Inject => "inject",
            Self::Recover => "recover",
            Self::Cleanup => "cleanup",
        }
    }
}

pub(in crate::native) struct EncodedRequest {
    pub(in crate::native) bytes: Vec<u8>,
    pub(in crate::native) sha256: String,
}

#[derive(Serialize)]
struct Envelope<'a> {
    schema_version: u8,
    request_sha256: &'a str,
    request: &'a RequestDocument<'a>,
}

#[derive(Serialize)]
struct RequestDocument<'a> {
    schema_version: u8,
    stage: &'static str,
    sequence: u8,
    instance_id: &'a str,
    subject: SubjectDocument<'a>,
    executor_sha256: &'a str,
    scenario: ScenarioDocument<'a>,
    limits: LimitsDocument,
    artifact_contract: &'static str,
}

#[derive(Serialize)]
struct SubjectDocument<'a> {
    id: &'a str,
    implementation: &'a str,
    build_sha256: String,
}

#[derive(Serialize)]
struct ScenarioDocument<'a> {
    id: &'a str,
    title: &'a str,
    fault: FaultDocument,
    expected_recovery: &'static str,
}

#[derive(Serialize)]
struct LimitsDocument {
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

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum FaultDocument {
    CommitCrash { boundary: &'static str, timing: &'static str },
    Corruption { target: &'static str },
    DiskExhaustion { scope: &'static str },
    DependencyDeath { dependency: &'static str },
    RetryExhaustion { dependency: &'static str },
    DaemonKill { phase: &'static str },
    HostReboot { phase: &'static str },
}

pub(in crate::native) fn encode_request(
    stage: Stage,
    sequence: u8,
    instance_id: &str,
    descriptor: &SubjectDescriptor,
    executor_sha256: &str,
    scenario: &ScenarioSpec,
    config: QualificationConfig,
) -> Result<EncodedRequest, crate::SubjectError> {
    let request = RequestDocument {
        schema_version: 1,
        stage: stage.code(),
        sequence,
        instance_id,
        subject: SubjectDocument {
            id: descriptor.id().as_str(),
            implementation: descriptor.implementation().as_str(),
            build_sha256: digest::hex(descriptor.build_digest()),
        },
        executor_sha256,
        scenario: ScenarioDocument {
            id: scenario.id().as_str(),
            title: scenario.title().as_str(),
            fault: fault_document(scenario.fault()),
            expected_recovery: recovery_code(scenario.expected_recovery()),
        },
        limits: limits_document(config),
        artifact_contract: "peritus-h1-retained-artifacts-v1",
    };
    let canonical = serde_json::to_vec(&request).map_err(|error| {
        subject_error(
            SubjectErrorCode::Observation,
            format!("serialize native controller request: {error}"),
            false,
        )
    })?;
    let sha256 = digest::hex(digest::bytes(&canonical));
    let mut bytes = serde_json::to_vec(&Envelope {
        schema_version: 1,
        request_sha256: &sha256,
        request: &request,
    })
    .map_err(|error| {
        subject_error(
            SubjectErrorCode::Observation,
            format!("serialize native controller envelope: {error}"),
            false,
        )
    })?;
    bytes.push(b'\n');
    Ok(EncodedRequest { bytes, sha256 })
}

const fn limits_document(config: QualificationConfig) -> LimitsDocument {
    let retries = config.retries();
    let resources = config.resources();
    LimitsDocument {
        max_scenarios: config.max_scenarios(),
        max_milestones: config.max_milestones_per_scenario(),
        provider_retries: retries.provider(),
        tool_retries: retries.tool(),
        worker_restarts: retries.worker(),
        reconciliation_steps: retries.reconciliation(),
        events: resources.events(),
        evidence_bytes: resources.evidence_bytes(),
        owned_processes: resources.owned_processes(),
        cleanup_steps: resources.cleanup_steps(),
        logical_ticks: resources.logical_ticks(),
    }
}

const fn fault_document(fault: FaultInjection) -> FaultDocument {
    use crate::{CrashTiming, FaultInjection as F};

    match fault {
        F::CommitCrash { boundary, timing } => FaultDocument::CommitCrash {
            boundary: boundary.code(),
            timing: match timing {
                CrashTiming::BeforeDurableCommit => "before-durable-commit",
                CrashTiming::AfterDurableCommitBeforeAck => "after-durable-commit-before-ack",
            },
        },
        F::Corruption(target) => FaultDocument::Corruption { target: target.code() },
        F::DiskExhaustion(scope) => FaultDocument::DiskExhaustion { scope: scope.code() },
        F::DependencyDeath(dependency) => {
            FaultDocument::DependencyDeath { dependency: dependency.code() }
        }
        F::RetryExhaustion(dependency) => {
            FaultDocument::RetryExhaustion { dependency: dependency.code() }
        }
        F::DaemonKill(phase) => FaultDocument::DaemonKill { phase: phase.code() },
        F::HostReboot(phase) => FaultDocument::HostReboot { phase: phase.code() },
    }
}

pub(super) const fn recovery_code(outcome: crate::RecoveryOutcome) -> &'static str {
    use crate::RecoveryOutcome as O;

    match outcome {
        O::RolledBackUncommitted => "rolled-back-uncommitted",
        O::ReplayedCommitted => "replayed-committed",
        O::RebuiltProjection => "rebuilt-projection",
        O::QuarantinedCorruption => "quarantined-corruption",
        O::FailedClosed => "failed-closed",
        O::DiscardedUnreferenced => "discarded-unreferenced",
        O::ReconciledOwnedWork => "reconciled-owned-work",
        O::RetryBudgetExhausted => "retry-budget-exhausted",
    }
}
