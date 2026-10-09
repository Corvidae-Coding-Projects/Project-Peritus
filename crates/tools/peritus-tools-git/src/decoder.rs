//! Strict schema-validated Git argument projection.

use peritus_tool_protocol::BoundedJson;
use peritus_types::SnapshotId;

use crate::{
    CandidateInput, DiffInput, GitToolError, GitToolOperation, HistoryInput, RollbackInput,
    SnapshotInput,
};

pub fn diff(value: &BoundedJson) -> Result<DiffInput, GitToolError> {
    DiffInput::new(
        string(value, "base_revision", GitToolOperation::Diff)?,
        number(value, "maximum_entries", GitToolOperation::Diff)?,
        number(value, "maximum_patch_bytes", GitToolOperation::Diff)?,
    )
}

pub fn history(value: &BoundedJson) -> Result<HistoryInput, GitToolError> {
    HistoryInput::new(number(value, "maximum_commits", GitToolOperation::History)?)
}

pub fn snapshot(value: &BoundedJson) -> Result<SnapshotInput, GitToolError> {
    match string(value, "kind", GitToolOperation::Snapshot)?.as_str() {
        "current" if value.property("snapshot_id").is_none() => Ok(SnapshotInput::Current),
        "retained" => Ok(SnapshotInput::Retained(snapshot_id(
            &string(value, "snapshot_id", GitToolOperation::Snapshot)?,
            GitToolOperation::Snapshot,
        )?)),
        _ => Err(invalid(GitToolOperation::Snapshot)),
    }
}

pub fn candidate(value: &BoundedJson) -> Result<CandidateInput, GitToolError> {
    Ok(CandidateInput::new(snapshot_id(
        &string(value, "snapshot_id", GitToolOperation::Candidate)?,
        GitToolOperation::Candidate,
    )?))
}

pub fn rollback(value: &BoundedJson) -> Result<RollbackInput, GitToolError> {
    RollbackInput::new(
        snapshot_id(
            &string(value, "target_snapshot_id", GitToolOperation::Rollback)?,
            GitToolOperation::Rollback,
        )?,
        snapshot_id(
            &string(value, "successor_snapshot_id", GitToolOperation::Rollback)?,
            GitToolOperation::Rollback,
        )?,
    )
}

fn string(
    value: &BoundedJson,
    name: &str,
    operation: GitToolOperation,
) -> Result<String, GitToolError> {
    value
        .property(name)
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| invalid(operation))
}

fn number<T>(
    value: &BoundedJson,
    name: &str,
    operation: GitToolOperation,
) -> Result<T, GitToolError>
where
    T: TryFrom<i64>,
{
    value
        .property(name)
        .and_then(|value| value.as_i64())
        .and_then(|value| T::try_from(value).ok())
        .ok_or_else(|| invalid(operation))
}

fn snapshot_id(value: &str, operation: GitToolOperation) -> Result<SnapshotId, GitToolError> {
    if value.len() != 32 {
        return Err(invalid(operation));
    }
    let mut bytes = [0_u8; 16];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = hex(pair[0])
            .and_then(|high| hex(pair[1]).map(|low| high * 16 + low))
            .ok_or_else(|| invalid(operation))?;
    }
    SnapshotId::new(bytes).map_err(|_| invalid(operation))
}

const fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

const fn invalid(operation: GitToolOperation) -> GitToolError {
    GitToolError::invalid(operation, "prepared Git arguments are structurally invalid")
}
