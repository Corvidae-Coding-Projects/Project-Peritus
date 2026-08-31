//! Controlled content-addressed artifact corruption and startup containment.

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
            OsStr::new("qualify-blob-corruption-stage"),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("inject.stdout"),
        &runtime.root.join("inject.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd artifact corruption failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let checkpoint = one_line(&output.stdout, "artifact corruption")?;
    let stage = parse_stage(&checkpoint)?;
    if stage.digest != stage.original_sha256
        || stage.original_sha256 == stage.corrupt_sha256
        || stage.bytes == 0
        || !stage.corruption_detected
    {
        return Err("artifact corruption checkpoint is not the exact injected fault".into());
    }
    Ok(InjectedCandidate {
        checkpoint,
        claim_fence: None,
        request_sha256: None,
        effect_path: None,
        effect_sha256: None,
        effect_bytes: None,
        artifact_sha256: Some(stage.digest),
        artifact_bytes: Some(stage.bytes),
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
            OsStr::new("qualify-blob-corruption-recover"),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("recover.stdout"),
        &runtime.root.join("recover.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd artifact containment failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let observation = one_line(&output.stdout, "artifact corruption recovery")?;
    let recovered = parse_recovery(&observation)?;
    let stage = parse_stage(&injected.checkpoint)?;
    let identity_preserved = recovered.digest == stage.digest;
    let corruption_preserved = recovered.quarantined_sha256 == stage.corrupt_sha256;
    let size_preserved = recovered.bytes == stage.bytes;
    if !identity_preserved || !corruption_preserved || !size_preserved {
        return Err("staged daemon did not quarantine the corrupt artifact".into());
    }
    independently_verify(runtime, &recovered)?;
    let journal = runtime.state.join("peritus.sqlite3");
    let metadata = fs::symlink_metadata(&journal)?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err("artifact containment journal is missing after recovery".into());
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
        effect_bytes: None,
        artifact_sha256: Some(recovered.quarantined_sha256),
        artifact_bytes: Some(recovered.bytes),
        snapshot: None,
        lease: None,
        patch: None,
        gate: None,
        promotion: None,
        projection: None,
        dependency: None,
        elapsed_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

fn independently_verify(
    runtime: &RuntimePaths,
    recovered: &RecoveryObservation,
) -> Result<(), Box<dyn std::error::Error>> {
    let artifact_root = runtime.state.join("artifacts");
    let suffix =
        std::path::Path::new("sha256").join(&recovered.digest[..2]).join(&recovered.digest);
    let object = artifact_root.join("objects").join(&suffix);
    let quarantine = artifact_root.join("quarantine").join(suffix);
    if object.exists() {
        return Err("corrupt bytes remained in the active object namespace".into());
    }
    let metadata = fs::symlink_metadata(&quarantine)?;
    if !metadata.file_type().is_file()
        || metadata.len() != recovered.bytes
        || digest::hex(digest::file(&quarantine)?) != recovered.quarantined_sha256
    {
        return Err("quarantined artifact differs from the injected corrupt bytes".into());
    }
    Ok(())
}

struct StageObservation {
    digest: String,
    original_sha256: String,
    corrupt_sha256: String,
    bytes: u64,
    corruption_detected: bool,
}

fn parse_stage(line: &str) -> Result<StageObservation, Box<dyn std::error::Error>> {
    let fields = fields(line, "peritus-qualification blob-corruption-stage ", 5)?;
    Ok(StageObservation {
        digest: sha256_field(fields[0], "digest")?,
        original_sha256: sha256_field(fields[1], "original_sha256")?,
        corrupt_sha256: sha256_field(fields[2], "corrupt_sha256")?,
        bytes: number_field(fields[3], "bytes")?,
        corruption_detected: boolean_field(fields[4], "corruption_detected")?,
    })
}

struct RecoveryObservation {
    digest: String,
    quarantined_sha256: String,
    bytes: u64,
}

fn parse_recovery(line: &str) -> Result<RecoveryObservation, Box<dyn std::error::Error>> {
    let fields = fields(line, "peritus-qualification blob-corruption-recover ", 7)?;
    let observation = RecoveryObservation {
        digest: sha256_field(fields[0], "digest")?,
        quarantined_sha256: sha256_field(fields[1], "quarantined_sha256")?,
        bytes: number_field(fields[2], "bytes")?,
    };
    let journal_verified = boolean_field(fields[3], "journal_verified")?;
    let reference_retained = boolean_field(fields[4], "reference_retained")?;
    let corruption_detected = boolean_field(fields[5], "corruption_detected")?;
    let mutation_admitted = boolean_field(fields[6], "mutation_admitted")?;
    if !journal_verified || !reference_retained || !corruption_detected || mutation_admitted {
        return Err("artifact containment observation did not fail closed".into());
    }
    Ok(observation)
}

fn fields<'a>(
    line: &'a str,
    prefix: &str,
    count: usize,
) -> Result<Vec<&'a str>, Box<dyn std::error::Error>> {
    let values = line
        .strip_prefix(prefix)
        .ok_or("staged peritusd returned an unknown artifact corruption observation")?
        .split_whitespace()
        .collect::<Vec<_>>();
    if values.len() != count {
        return Err("artifact corruption observation has the wrong field count".into());
    }
    Ok(values)
}

fn sha256_field(field: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = value_field(field, name)?;
    if !lower_sha256(value) {
        return Err(format!("artifact corruption field {name} is not canonical SHA-256").into());
    }
    Ok(value.to_owned())
}

fn boolean_field(field: &str, name: &str) -> Result<bool, Box<dyn std::error::Error>> {
    match value_field(field, name)? {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("artifact corruption field {name} is not boolean").into()),
    }
}

fn number_field(field: &str, name: &str) -> Result<u64, Box<dyn std::error::Error>> {
    value_field(field, name)?.parse::<u64>().map_err(Into::into)
}

fn value_field<'a>(field: &'a str, name: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    let (observed, value) = field.split_once('=').ok_or("malformed artifact corruption field")?;
    if observed != name || value.is_empty() {
        return Err(format!("expected artifact corruption field {name}, found {observed}").into());
    }
    Ok(value)
}
