//! Strict decoding of the quality-owned structured terminal projection.

use peritus_tool_protocol::ResultStatus;
use peritus_types::{GateId, ProcessId, Sha256Digest};

#[derive(Clone, Copy)]
pub(super) struct DecodedStructured {
    pub(super) gate_id: GateId,
    pub(super) outcome: DecodedOutcome,
    pub(super) result_digest: Sha256Digest,
    pub(super) plan_digest: Sha256Digest,
    pub(super) process_id: ProcessId,
    pub(super) execution_complete: bool,
    pub(super) progress_truncated: bool,
    pub(super) expected_status: Option<ResultStatus>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum DecodedOutcome {
    Passed,
    PredicateFailed,
    UnsuccessfulExit,
    InvalidResult,
    Infrastructure,
}

pub(super) fn decode_structured(
    value: &peritus_tool_protocol::BoundedJson,
) -> Option<DecodedStructured> {
    let candidate = value.property("candidate")?;
    let execution = value.property("execution")?;
    let outcome = match candidate.property("outcome")?.as_str()? {
        "passed" => DecodedOutcome::Passed,
        "predicate-failed" => DecodedOutcome::PredicateFailed,
        "unsuccessful-exit" => DecodedOutcome::UnsuccessfulExit,
        "invalid-result" => DecodedOutcome::InvalidResult,
        "infrastructure" => DecodedOutcome::Infrastructure,
        _ => return None,
    };
    let execution_complete = execution.property("complete")?.as_bool()?;
    let expected_status = match value.property("classification_version") {
        None => None,
        Some(version) if version.as_u64() == Some(2) => {
            Some(current_status(&execution, outcome, execution_complete)?)
        }
        Some(_) => return None,
    };
    Some(DecodedStructured {
        gate_id: GateId::new(hex_array(candidate.property("gate_id")?.as_str()?)?).ok()?,
        outcome,
        result_digest: Sha256Digest::new(hex_array(
            candidate.property("result_digest")?.as_str()?,
        )?),
        plan_digest: Sha256Digest::new(hex_array(execution.property("plan_digest")?.as_str()?)?),
        process_id: ProcessId::new(hex_array(execution.property("process_id")?.as_str()?)?).ok()?,
        execution_complete,
        progress_truncated: value.property("progress_truncated")?.as_bool()?,
        expected_status,
    })
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum DecodedDisposition {
    Exited,
    Signalled,
    SpawnFailed,
    Cancelled,
    TimedOut,
    OutputLimit,
    ResourceLimit,
    SandboxDenied,
    SupervisorFailed,
    RecoveryIndeterminate,
}

fn current_status(
    execution: &peritus_tool_protocol::BoundedJson,
    outcome: DecodedOutcome,
    execution_complete: bool,
) -> Option<ResultStatus> {
    let disposition = match execution.property("disposition")?.as_str()? {
        "exited" => DecodedDisposition::Exited,
        "signalled" => DecodedDisposition::Signalled,
        "spawn-failed" => DecodedDisposition::SpawnFailed,
        "cancelled" => DecodedDisposition::Cancelled,
        "timed-out" => DecodedDisposition::TimedOut,
        "output-limit" => DecodedDisposition::OutputLimit,
        "resource-limit" => DecodedDisposition::ResourceLimit,
        "sandbox-denied" => DecodedDisposition::SandboxDenied,
        "supervisor-failed" => DecodedDisposition::SupervisorFailed,
        "recovery-indeterminate" => DecodedDisposition::RecoveryIndeterminate,
        _ => return None,
    };
    execution.property("os_exit")?.as_str()?;
    let output_complete = execution.property("output_complete")?.as_bool()?;
    let artifact_complete = execution.property("artifact_publication_complete")?.as_bool()?;
    let cleanup_complete = execution.property("cleanup_complete")?.as_bool()?;
    let parser_complete = execution.property("parser_complete")?.as_bool()?;
    let predicate_satisfied = execution.property("predicate_satisfied")?.as_bool()?;
    let expected_exit_satisfied = execution.property("expected_exit_satisfied")?.as_bool()?;
    if execution_complete
        != (output_complete && artifact_complete && cleanup_complete && parser_complete)
        || !parser_complete && predicate_satisfied
    {
        return None;
    }
    let (derived_outcome, status) = match disposition {
        DecodedDisposition::TimedOut => {
            (DecodedOutcome::Infrastructure, ResultStatus::TimedOut)
        }
        DecodedDisposition::Cancelled => {
            (DecodedOutcome::Infrastructure, ResultStatus::Cancelled)
        }
        DecodedDisposition::RecoveryIndeterminate => {
            (DecodedOutcome::Infrastructure, ResultStatus::Indeterminate)
        }
        _ if !(output_complete && artifact_complete && cleanup_complete)
            || disposition != DecodedDisposition::Exited =>
        {
            (DecodedOutcome::Infrastructure, ResultStatus::Failed)
        }
        _ if !parser_complete => (DecodedOutcome::InvalidResult, ResultStatus::Failed),
        _ if !predicate_satisfied => (DecodedOutcome::PredicateFailed, ResultStatus::Failed),
        _ if expected_exit_satisfied => (DecodedOutcome::Passed, ResultStatus::Succeeded),
        _ => (DecodedOutcome::UnsuccessfulExit, ResultStatus::Failed),
    };
    (derived_outcome == outcome).then_some(status)
}

fn hex_array<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N.checked_mul(2)? {
        return None;
    }
    let mut bytes = [0_u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = nibble(pair[0])?.checked_mul(16)?.checked_add(nibble(pair[1])?)?;
    }
    Some(bytes)
}

const fn nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}
