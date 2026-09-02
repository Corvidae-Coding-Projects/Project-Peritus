//! Controlled journal-frame corruption followed by real fail-closed daemon startup.

use std::ffi::OsStr;
use std::fs;
use std::time::Instant;

use crate::digest;

use super::process::{bounded_command, one_line};
use super::{InjectedCandidate, RecoveredCandidate, RuntimePaths};
use crate::native_controller::args::{ControllerPaths, lower_sha256};

pub(super) fn inject(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
) -> Result<InjectedCandidate, Box<dyn std::error::Error>> {
    let output = bounded_command(
        &paths.candidate,
        [
            OsStr::new("qualify-journal-corruption-stage"),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("inject.stdout"),
        &runtime.root.join("inject.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd journal corruption failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let checkpoint = one_line(&output.stdout, "journal corruption")?;
    let stage = parse_stage(&checkpoint)?;
    if stage.original_frame_sha256 == stage.corrupt_frame_sha256
        || stage.event_count != 1
        || !stage.corruption_detected
    {
        return Err("journal corruption checkpoint is not the exact injected fault".into());
    }
    Ok(InjectedCandidate {
        checkpoint,
        claim_fence: None,
        request_sha256: Some(stage.request_sha256),
        effect_path: None,
        effect_sha256: None,
        effect_bytes: None,
        artifact_sha256: None,
        artifact_bytes: None,
        snapshot: None,
        lease: None,
        patch: None,
        gate: None,
        promotion: None,
        projection: None,
        fault_process_exit: output.status.to_string(),
    })
}

pub(super) fn recover(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
    injected: &InjectedCandidate,
) -> Result<RecoveredCandidate, Box<dyn std::error::Error>> {
    let started = Instant::now();
    let output = bounded_command(
        &paths.candidate,
        [
            OsStr::new("qualify-journal-corruption-recover"),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("recover.stdout"),
        &runtime.root.join("recover.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd journal fail-closed recovery failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let observation = one_line(&output.stdout, "journal corruption recovery")?;
    let recovered = parse_recovery(&observation)?;
    let stage = parse_stage(&injected.checkpoint)?;
    if recovered.startup_error_code != "PERITUS-DAEMON-STATE-001"
        || recovered.corrupt_frame_sha256 != stage.corrupt_frame_sha256
        || recovered.event_count != 1
        || recovered.aggregate_heads != 1
        || recovered.state_records != 1
        || recovered.authority_epochs != 0
        || recovered.application_principals != 0
        || !recovered.corruption_detected
        || recovered.mutation_admitted
    {
        return Err("staged daemon did not fail closed on the corrupt journal".into());
    }
    let journal = runtime.state.join("peritus.sqlite3");
    let metadata = fs::symlink_metadata(&journal)?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err("corrupt authoritative journal is missing after failed startup".into());
    }
    Ok(RecoveredCandidate {
        observation,
        destination_reconciled: true,
        external_effects: 0,
        duplicate_effects: 0,
        exact_fence_acknowledged: false,
        pending_claims: 0,
        committed_events: Some(recovered.event_count),
        aggregate_heads: Some(recovered.aggregate_heads),
        journal_sha256: digest::hex(digest::file(&journal)?),
        journal_bytes: metadata.len(),
        effect_sha256: None,
        effect_bytes: None,
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
    original_frame_sha256: String,
    corrupt_frame_sha256: String,
    event_count: u64,
    corruption_detected: bool,
}

fn parse_stage(line: &str) -> Result<StageObservation, Box<dyn std::error::Error>> {
    let values = fields(line, "peritus-qualification journal-corruption-stage ", 5)?;
    Ok(StageObservation {
        request_sha256: sha256_field(values[0], "request_sha256")?,
        original_frame_sha256: sha256_field(values[1], "original_frame_sha256")?,
        corrupt_frame_sha256: sha256_field(values[2], "corrupt_frame_sha256")?,
        event_count: number_field(values[3], "event_count")?,
        corruption_detected: boolean_field(values[4], "corruption_detected")?,
    })
}

struct RecoveryObservation {
    startup_error_code: String,
    corrupt_frame_sha256: String,
    event_count: u64,
    aggregate_heads: u64,
    state_records: u64,
    authority_epochs: u64,
    application_principals: u64,
    corruption_detected: bool,
    mutation_admitted: bool,
}

fn parse_recovery(line: &str) -> Result<RecoveryObservation, Box<dyn std::error::Error>> {
    let values = fields(line, "peritus-qualification journal-corruption-recover ", 9)?;
    Ok(RecoveryObservation {
        startup_error_code: text_field(values[0], "startup_error_code")?.to_owned(),
        corrupt_frame_sha256: sha256_field(values[1], "corrupt_frame_sha256")?,
        event_count: number_field(values[2], "event_count")?,
        aggregate_heads: number_field(values[3], "aggregate_heads")?,
        state_records: number_field(values[4], "state_records")?,
        authority_epochs: number_field(values[5], "authority_epochs")?,
        application_principals: number_field(values[6], "application_principals")?,
        corruption_detected: boolean_field(values[7], "corruption_detected")?,
        mutation_admitted: boolean_field(values[8], "mutation_admitted")?,
    })
}

fn fields<'a>(
    line: &'a str,
    prefix: &str,
    count: usize,
) -> Result<Vec<&'a str>, Box<dyn std::error::Error>> {
    let fields = line
        .strip_prefix(prefix)
        .ok_or("staged peritusd returned an unknown journal corruption observation")?
        .split_whitespace()
        .collect::<Vec<_>>();
    if fields.len() != count {
        return Err("journal corruption observation has the wrong field count".into());
    }
    Ok(fields)
}

fn sha256_field(field: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = text_field(field, name)?;
    if !lower_sha256(value) {
        return Err(format!("journal corruption field {name} is not canonical SHA-256").into());
    }
    Ok(value.to_owned())
}

fn boolean_field(field: &str, name: &str) -> Result<bool, Box<dyn std::error::Error>> {
    match text_field(field, name)? {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("journal corruption field {name} is not boolean").into()),
    }
}

fn number_field(field: &str, name: &str) -> Result<u64, Box<dyn std::error::Error>> {
    text_field(field, name)?.parse::<u64>().map_err(Into::into)
}

fn text_field<'a>(field: &'a str, name: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    let (observed, value) = field.split_once('=').ok_or("malformed journal corruption field")?;
    if observed != name || value.is_empty() {
        return Err(format!("expected journal corruption field {name}, found {observed}").into());
    }
    Ok(value)
}
