//! Staged-daemon effects for content-addressed artifact commit recovery.

use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::time::Instant;

use crate::digest;

use super::process::{bounded_command, kill_after_checkpoint, one_line};
use super::{InjectedCandidate, RecoveredCandidate, RuntimePaths};
use crate::native_controller::args::{ControllerPaths, lower_sha256};
use crate::native_controller::request::ScenarioRoute;

pub(super) fn inject(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
    route: ScenarioRoute,
) -> Result<InjectedCandidate, Box<dyn std::error::Error>> {
    let (command, prefix, field_count) = match route {
        ScenarioRoute::BlobBeforeDurableCommit => {
            ("qualify-blob-before-stage", "peritus-qualification blob-before-stage ", 3)
        }
        ScenarioRoute::BlobAfterDurableCommitBeforeAck => {
            ("qualify-blob-after-stage", "peritus-qualification blob-after-stage ", 4)
        }
        _ => return Err("blob controller received a non-blob route".into()),
    };
    let killed = kill_after_checkpoint(
        &paths.candidate,
        [OsStr::new(command), OsStr::new("--config"), runtime.config.as_os_str()],
        &runtime.root,
        &runtime.root.join("inject.stderr"),
    )?;
    let fields = fields(&killed.line, prefix, field_count)?;
    let artifact_sha256 = value_field(fields[0], "digest")?.to_owned();
    if !lower_sha256(&artifact_sha256) {
        return Err("staged peritusd returned an invalid artifact digest".into());
    }
    let artifact_bytes = number_field(fields[1], "bytes")?;
    match route {
        ScenarioRoute::BlobBeforeDurableCommit => {
            if number_field(fields[2], "temporary_files")? != 1 {
                return Err("pre-commit artifact checkpoint has no exact temporary object".into());
            }
        }
        ScenarioRoute::BlobAfterDurableCommitBeforeAck => {
            if !boolean_field(fields[2], "finalized")? || !boolean_field(fields[3], "referenced")? {
                return Err("post-commit artifact checkpoint is not durable and referenced".into());
            }
        }
        _ => return Err("blob controller received a non-blob route".into()),
    }
    Ok(InjectedCandidate {
        checkpoint: killed.line,
        claim_fence: None,
        request_sha256: None,
        effect_path: None,
        effect_sha256: None,
        effect_bytes: None,
        artifact_sha256: Some(artifact_sha256),
        artifact_bytes: Some(artifact_bytes),
        snapshot: None,
        lease: None,
        patch: None,
        gate: None,
        promotion: None,
        projection: None,
        fault_process_exit: killed.status,
    })
}

pub(super) fn recover(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
    injected: &InjectedCandidate,
    route: ScenarioRoute,
) -> Result<RecoveredCandidate, Box<dyn std::error::Error>> {
    let started = Instant::now();
    let (command, prefix, committed) = match route {
        ScenarioRoute::BlobBeforeDurableCommit => {
            ("qualify-blob-before-recover", "peritus-qualification blob-before-recover ", false)
        }
        ScenarioRoute::BlobAfterDurableCommitBeforeAck => {
            ("qualify-blob-after-recover", "peritus-qualification blob-after-recover ", true)
        }
        _ => return Err("blob controller received a non-blob route".into()),
    };
    let output = bounded_command(
        &paths.candidate,
        [OsStr::new(command), OsStr::new("--config"), runtime.config.as_os_str()],
        &runtime.root,
        &runtime.root.join("recover.stdout"),
        &runtime.root.join("recover.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd artifact recovery failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let observation = one_line(&output.stdout, "artifact recovery")?;
    let values = fields(&observation, prefix, 7)?;
    let artifact_sha256 = value_field(values[0], "digest")?.to_owned();
    let artifact_bytes = number_field(values[1], "bytes")?;
    let journal_verified = boolean_field(values[2], "journal_verified")?;
    let finalized = boolean_field(values[3], "finalized")?;
    let referenced = boolean_field(values[4], "referenced")?;
    let temporary_files = number_field(values[5], "temporary_files")?;
    let object_files = number_field(values[6], "object_files")?;
    if Some(&artifact_sha256) != injected.artifact_sha256.as_ref()
        || Some(artifact_bytes) != injected.artifact_bytes
        || !journal_verified
        || finalized != committed
        || referenced != committed
        || temporary_files != 0
        || object_files != u64::from(committed)
    {
        return Err("staged peritusd artifact recovery differs from the commit boundary".into());
    }
    independently_verify(runtime, &artifact_sha256, artifact_bytes, committed)?;
    let journal = runtime.state.join("peritus.sqlite3");
    let journal_metadata = fs::symlink_metadata(&journal)?;
    if !journal_metadata.file_type().is_file() || journal_metadata.len() == 0 {
        return Err("artifact recovery journal is missing or empty".into());
    }
    Ok(RecoveredCandidate {
        observation,
        destination_reconciled: true,
        external_effects: object_files,
        duplicate_effects: 0,
        exact_fence_acknowledged: false,
        pending_claims: 0,
        committed_events: Some(0),
        aggregate_heads: Some(0),
        journal_sha256: digest::hex(digest::file(&journal)?),
        journal_bytes: journal_metadata.len(),
        effect_sha256: None,
        effect_bytes: None,
        artifact_sha256: committed.then_some(artifact_sha256),
        artifact_bytes: committed.then_some(artifact_bytes),
        snapshot: None,
        lease: None,
        patch: None,
        gate: None,
        promotion: None,
        projection: None,
        elapsed_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

fn independently_verify(
    runtime: &RuntimePaths,
    sha256: &str,
    bytes: u64,
    committed: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !lower_sha256(sha256) || sha256.len() < 2 {
        return Err("artifact recovery digest is not canonical".into());
    }
    let artifact_root = runtime.state.join("artifacts");
    let object = artifact_root.join("objects").join("sha256").join(&sha256[..2]).join(sha256);
    match fs::symlink_metadata(&object) {
        Ok(metadata) if committed && metadata.file_type().is_file() && metadata.len() == bytes => {
            if digest::hex(digest::file(&object)?) != sha256 {
                return Err("recovered artifact bytes differ from their digest".into());
            }
        }
        Err(error) if !committed && error.kind() == std::io::ErrorKind::NotFound => {}
        _ => {
            return Err(
                "recovered artifact object presence differs from the commit boundary".into()
            );
        }
    }
    if count_files(&artifact_root.join("temporary"))? != 0 {
        return Err("artifact recovery left temporary bytes".into());
    }
    Ok(())
}

fn count_files(path: &Path) -> Result<u64, Box<dyn std::error::Error>> {
    let mut count = 0_u64;
    for entry in fs::read_dir(path)? {
        if entry?.file_type()?.is_file() {
            count = count.checked_add(1).ok_or("artifact file count overflow")?;
        }
    }
    Ok(count)
}

fn fields<'a>(
    line: &'a str,
    prefix: &str,
    count: usize,
) -> Result<Vec<&'a str>, Box<dyn std::error::Error>> {
    let fields = line
        .strip_prefix(prefix)
        .ok_or("staged peritusd returned an unknown artifact observation")?
        .split_whitespace()
        .collect::<Vec<_>>();
    if fields.len() != count {
        return Err("artifact observation has the wrong field count".into());
    }
    Ok(fields)
}

fn boolean_field(field: &str, name: &str) -> Result<bool, Box<dyn std::error::Error>> {
    match value_field(field, name)? {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("staged peritusd field {name} is not boolean").into()),
    }
}

fn number_field(field: &str, name: &str) -> Result<u64, Box<dyn std::error::Error>> {
    value_field(field, name)?.parse::<u64>().map_err(Into::into)
}

fn value_field<'a>(field: &'a str, name: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    let (observed, value) = field.split_once('=').ok_or("malformed staged peritusd field")?;
    if observed != name {
        return Err(format!("expected staged peritusd field {name}, found {observed}").into());
    }
    Ok(value)
}
