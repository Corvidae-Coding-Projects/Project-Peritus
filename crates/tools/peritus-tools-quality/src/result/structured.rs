//! Strict decoding of the quality-owned structured terminal projection.

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
}

#[derive(Clone, Copy)]
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
    Some(DecodedStructured {
        gate_id: GateId::new(hex_array(candidate.property("gate_id")?.as_str()?)?).ok()?,
        outcome: match candidate.property("outcome")?.as_str()? {
            "passed" => DecodedOutcome::Passed,
            "predicate-failed" => DecodedOutcome::PredicateFailed,
            "unsuccessful-exit" => DecodedOutcome::UnsuccessfulExit,
            "invalid-result" => DecodedOutcome::InvalidResult,
            "infrastructure" => DecodedOutcome::Infrastructure,
            _ => return None,
        },
        result_digest: Sha256Digest::new(hex_array(
            candidate.property("result_digest")?.as_str()?,
        )?),
        plan_digest: Sha256Digest::new(hex_array(execution.property("plan_digest")?.as_str()?)?),
        process_id: ProcessId::new(hex_array(execution.property("process_id")?.as_str()?)?).ok()?,
        execution_complete: execution.property("complete")?.as_bool()?,
        progress_truncated: value.property("progress_truncated")?.as_bool()?,
    })
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
