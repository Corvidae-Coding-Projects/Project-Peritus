//! Exact operation envelopes and validation, independent of effect ownership.
use super::{Envelope, Operation, RECORD_VERSION, Result, problem};
use std::path::Path;

pub(super) fn envelope(operation: &str, record: Operation) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec_pretty(&Envelope {
        schema_version: RECORD_VERSION,
        operation: operation.to_owned(),
        record,
    })?)
}

pub(super) fn read_record(path: &Path, expected: &str) -> Result<Option<Operation>> {
    let Some(envelope) = read_envelope(path)? else { return Ok(None) };
    if envelope.schema_version != RECORD_VERSION {
        return Err(problem("An operation record uses an unsupported schema"));
    }
    if envelope.operation != expected {
        return Err(problem(
            "An operation record is stored under another durable identity",
        ));
    }
    Ok(Some(envelope.record))
}

pub(super) fn read_envelope(path: &Path) -> Result<Option<Envelope>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let envelope: Envelope = serde_json::from_slice(&bytes).map_err(|error| {
        problem(format!(
            "Operation record at {} is unreadable and was preserved: {error}",
            path.display()
        ))
    })?;
    Ok(Some(envelope))
}

pub(super) fn same_record(left: &Operation, right: &Operation) -> bool {
    left.input == right.input && left.prepared == right.prepared && left.result == right.result
}

pub(super) fn same_optional(left: &Option<Operation>, right: &Option<Operation>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => same_record(left, right),
        (None, None) => true,
        _ => false,
    }
}
