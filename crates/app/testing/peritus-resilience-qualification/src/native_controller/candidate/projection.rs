//! Controlled projection corruption followed by real staged-daemon startup repair.

use std::ffi::OsStr;
use std::fs;
use std::time::Instant;

use crate::digest;

use super::process::{bounded_command, one_line};
use super::{
    InjectedCandidate, ProjectionCorruptionCheckpoint, ProjectionRepairObservation,
    RecoveredCandidate, RuntimePaths,
};
use crate::native_controller::args::{ControllerPaths, lower_sha256};

pub(super) fn inject(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
) -> Result<InjectedCandidate, Box<dyn std::error::Error>> {
    let output = bounded_command(
        &paths.candidate,
        [
            OsStr::new("qualify-projection-corruption-stage"),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("inject.stdout"),
        &runtime.root.join("inject.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd projection corruption failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let checkpoint_line = one_line(&output.stdout, "projection corruption")?;
    let values = fields(&checkpoint_line, "peritus-qualification projection-corruption-stage ", 6)?;
    let checkpoint = ProjectionCorruptionCheckpoint {
        name: name_field(values[0], "projection")?,
        generation: number_field(values[1], "generation")?,
        original_payload_sha256: sha256_field(values[2], "original_payload_sha256")?,
        corrupt_payload_sha256: sha256_field(values[3], "corrupt_payload_sha256")?,
        payload_bytes: number_field(values[4], "payload_bytes")?,
        corrupted: boolean_field(values[5], "corrupted")?,
    };
    if checkpoint.generation != 1
        || checkpoint.original_payload_sha256 == checkpoint.corrupt_payload_sha256
        || checkpoint.payload_bytes == 0
        || !checkpoint.corrupted
    {
        return Err("projection corruption checkpoint is not the exact injected fault".into());
    }
    Ok(InjectedCandidate {
        checkpoint: checkpoint_line,
        claim_fence: None,
        request_sha256: None,
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
        projection: Some(checkpoint),
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
            OsStr::new("qualify-projection-corruption-recover"),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("recover.stdout"),
        &runtime.root.join("recover.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd projection repair failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let observation = one_line(&output.stdout, "projection repair")?;
    let values = fields(&observation, "peritus-qualification projection-corruption-recover ", 11)?;
    let repaired = ProjectionRepairObservation {
        name: name_field(values[0], "projection")?,
        previous_generation: number_field(values[1], "previous_generation")?,
        repaired_generation: number_field(values[2], "repaired_generation")?,
        corrupt_payload_sha256: sha256_field(values[3], "corrupt_payload_sha256")?,
        repaired_payload_sha256: sha256_field(values[4], "repaired_payload_sha256")?,
        payload_bytes: number_field(values[5], "payload_bytes")?,
        generation_count: number_field(values[6], "generation_count")?,
        event_count: number_field(values[7], "event_count")?,
        aggregate_heads: number_field(values[8], "aggregate_heads")?,
        payload_valid: boolean_field(values[9], "payload_valid")?,
        reusable: boolean_field(values[10], "reusable")?,
    };
    verify_repair(injected, &repaired)?;
    let journal = runtime.state.join("peritus.sqlite3");
    let metadata = fs::symlink_metadata(&journal)?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err("projection recovery journal is missing or empty".into());
    }
    Ok(RecoveredCandidate {
        observation,
        destination_reconciled: true,
        external_effects: 0,
        duplicate_effects: 0,
        exact_fence_acknowledged: false,
        pending_claims: 0,
        committed_events: Some(repaired.event_count),
        aggregate_heads: Some(repaired.aggregate_heads),
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
        projection: Some(repaired),
        elapsed_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

fn verify_repair(
    injected: &InjectedCandidate,
    repaired: &ProjectionRepairObservation,
) -> Result<(), Box<dyn std::error::Error>> {
    let checkpoint =
        injected.projection.as_ref().ok_or("projection corruption checkpoint is absent")?;
    let generation_advanced = repaired.previous_generation == checkpoint.generation
        && repaired.repaired_generation == checkpoint.generation.saturating_add(1);
    let corrupt_digest_preserved =
        repaired.corrupt_payload_sha256 == checkpoint.corrupt_payload_sha256;
    let original_digest_restored =
        repaired.repaired_payload_sha256 == checkpoint.original_payload_sha256;
    let payload_size_restored = repaired.payload_bytes == checkpoint.payload_bytes;
    if repaired.name != checkpoint.name
        || !generation_advanced
        || !corrupt_digest_preserved
        || !original_digest_restored
        || !payload_size_restored
        || repaired.generation_count != 2
        || repaired.event_count != 0
        || repaired.aggregate_heads != 0
        || !repaired.payload_valid
        || !repaired.reusable
    {
        return Err("staged daemon did not replace and verify the corrupt projection".into());
    }
    Ok(())
}

fn fields<'a>(
    line: &'a str,
    prefix: &str,
    count: usize,
) -> Result<Vec<&'a str>, Box<dyn std::error::Error>> {
    let fields = line
        .strip_prefix(prefix)
        .ok_or("staged peritusd returned an unknown projection observation")?
        .split_whitespace()
        .collect::<Vec<_>>();
    if fields.len() != count {
        return Err("projection observation has the wrong field count".into());
    }
    Ok(fields)
}

fn name_field(field: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = value_field(field, name)?;
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err("projection name is not canonical".into());
    }
    Ok(value.to_owned())
}

fn sha256_field(field: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = value_field(field, name)?;
    if !lower_sha256(value) {
        return Err(format!("projection field {name} is not canonical SHA-256").into());
    }
    Ok(value.to_owned())
}

fn boolean_field(field: &str, name: &str) -> Result<bool, Box<dyn std::error::Error>> {
    match value_field(field, name)? {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("projection field {name} is not boolean").into()),
    }
}

fn number_field(field: &str, name: &str) -> Result<u64, Box<dyn std::error::Error>> {
    value_field(field, name)?.parse::<u64>().map_err(Into::into)
}

fn value_field<'a>(field: &'a str, name: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    let (observed, value) = field.split_once('=').ok_or("malformed projection field")?;
    if observed != name || value.is_empty() {
        return Err(format!("expected projection field {name}, found {observed}").into());
    }
    Ok(value)
}
