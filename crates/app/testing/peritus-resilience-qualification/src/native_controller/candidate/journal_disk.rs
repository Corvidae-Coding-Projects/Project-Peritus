//! Authoritative-journal storage exhaustion through the staged production daemon.

use std::ffi::OsStr;
use std::fs;
use std::time::Instant;

use crate::digest;
use crate::native_controller::args::{ControllerPaths, lower_sha256};

use super::process::{bounded_command, one_line};
use super::{InjectedCandidate, RecoveredCandidate, RuntimePaths};

pub(super) fn inject(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
) -> Result<InjectedCandidate, Box<dyn std::error::Error>> {
    let output = bounded_command(
        &paths.candidate,
        [
            OsStr::new("qualify-disk-journal-append-stage"),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("inject.stdout"),
        &runtime.root.join("inject.stderr"),
    )?;
    require_success(&output, "journal storage injection")?;
    let checkpoint = one_line(&output.stdout, "journal storage injection")?;
    let stage = parse_stage(&checkpoint)?;
    if stage.page_count == 0
        || stage.page_size == 0
        || stage.maximum_bytes != stage.page_count.saturating_mul(stage.page_size)
        || !stage.storage_exhausted
        || !stage.append_absent
    {
        return Err("journal storage checkpoint did not prove exact exhaustion rollback".into());
    }
    Ok(InjectedCandidate {
        checkpoint,
        claim_fence: None,
        request_sha256: Some(stage.request_sha256),
        effect_path: None,
        effect_sha256: None,
        effect_bytes: Some(stage.maximum_bytes),
        artifact_sha256: None,
        artifact_bytes: None,
        snapshot: None,
        lease: None,
        patch: None,
        gate: None,
        promotion: None,
        projection: None,
        fault_process_exit: "journal-storage-exhausted".to_owned(),
    })
}

pub(super) fn recover(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
    injected: &InjectedCandidate,
) -> Result<RecoveredCandidate, Box<dyn std::error::Error>> {
    let started = Instant::now();
    let stage = parse_stage(&injected.checkpoint)?;
    let output = bounded_command(
        &paths.candidate,
        [
            OsStr::new("qualify-disk-journal-append-recover"),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("recover.stdout"),
        &runtime.root.join("recover.stderr"),
    )?;
    require_success(&output, "journal storage recovery")?;
    let observation = one_line(&output.stdout, "journal storage recovery")?;
    let recovered = parse_recovery(&observation)?;
    if recovered.request_sha256 != stage.request_sha256
        || recovered.page_count != stage.page_count
        || recovered.page_size != stage.page_size
        || recovered.maximum_bytes < stage.maximum_bytes
        || recovered.committed_events != 0
        || recovered.aggregate_heads != 0
        || !recovered.journal_verified
        || !recovered.append_absent
    {
        return Err("fresh journal storage recovery differs from the rejected append".into());
    }
    let journal = runtime.state.join("peritus.sqlite3");
    let metadata = fs::symlink_metadata(&journal)?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err("journal storage recovery database is absent".into());
    }
    Ok(RecoveredCandidate {
        observation,
        destination_reconciled: true,
        external_effects: 0,
        duplicate_effects: 0,
        exact_fence_acknowledged: false,
        pending_claims: 0,
        committed_events: Some(0),
        aggregate_heads: Some(0),
        journal_sha256: digest::hex(digest::file(&journal)?),
        journal_bytes: metadata.len(),
        effect_sha256: None,
        effect_bytes: Some(stage.maximum_bytes),
        artifact_sha256: None,
        artifact_bytes: None,
        snapshot: None,
        lease: None,
        patch: None,
        gate: None,
        promotion: None,
        projection: None,
        dependency: None,
        lifecycle: None,
        elapsed_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

struct StageObservation {
    request_sha256: String,
    page_count: u64,
    page_size: u64,
    maximum_bytes: u64,
    storage_exhausted: bool,
    append_absent: bool,
}

struct RecoveryObservation {
    request_sha256: String,
    page_count: u64,
    page_size: u64,
    maximum_bytes: u64,
    committed_events: u64,
    aggregate_heads: u64,
    journal_verified: bool,
    append_absent: bool,
}

fn parse_stage(line: &str) -> Result<StageObservation, Box<dyn std::error::Error>> {
    let fields = values(line, "peritus-qualification disk-journal-append-stage ", 6)?;
    Ok(StageObservation {
        request_sha256: sha256(fields[0], "request_sha256")?,
        page_count: number(fields[1], "page_count")?,
        page_size: number(fields[2], "page_size")?,
        maximum_bytes: number(fields[3], "maximum_bytes")?,
        storage_exhausted: boolean(fields[4], "storage_exhausted")?,
        append_absent: boolean(fields[5], "append_absent")?,
    })
}

fn parse_recovery(line: &str) -> Result<RecoveryObservation, Box<dyn std::error::Error>> {
    let fields = values(line, "peritus-qualification disk-journal-append-recover ", 8)?;
    Ok(RecoveryObservation {
        request_sha256: sha256(fields[0], "request_sha256")?,
        page_count: number(fields[1], "page_count")?,
        page_size: number(fields[2], "page_size")?,
        maximum_bytes: number(fields[3], "maximum_bytes")?,
        committed_events: number(fields[4], "committed_events")?,
        aggregate_heads: number(fields[5], "aggregate_heads")?,
        journal_verified: boolean(fields[6], "journal_verified")?,
        append_absent: boolean(fields[7], "append_absent")?,
    })
}

fn values<'a>(
    line: &'a str,
    prefix: &str,
    count: usize,
) -> Result<Vec<&'a str>, Box<dyn std::error::Error>> {
    let fields = line
        .strip_prefix(prefix)
        .ok_or("staged peritusd returned an unknown journal storage observation")?
        .split_ascii_whitespace()
        .collect::<Vec<_>>();
    if fields.len() == count {
        Ok(fields)
    } else {
        Err("journal storage observation has the wrong field count".into())
    }
}

fn field<'a>(value: &'a str, name: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    let (observed, text) = value.split_once('=').ok_or("malformed journal storage field")?;
    if observed == name && !text.is_empty() {
        Ok(text)
    } else {
        Err(format!("expected journal storage field {name}, found {observed}").into())
    }
}

fn sha256(value: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = field(value, name)?;
    if lower_sha256(value) {
        Ok(value.to_owned())
    } else {
        Err(format!("journal storage field {name} is not SHA-256").into())
    }
}

fn number<T: std::str::FromStr>(value: &str, name: &str) -> Result<T, Box<dyn std::error::Error>>
where
    T::Err: std::error::Error + 'static,
{
    Ok(field(value, name)?.parse::<T>()?)
}

fn boolean(value: &str, name: &str) -> Result<bool, Box<dyn std::error::Error>> {
    match field(value, name)? {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("journal storage field {name} is not boolean").into()),
    }
}

fn require_success(
    output: &super::process::CommandOutput,
    operation: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if output.status.success() && output.stderr.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "staged peritusd {operation} failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}
