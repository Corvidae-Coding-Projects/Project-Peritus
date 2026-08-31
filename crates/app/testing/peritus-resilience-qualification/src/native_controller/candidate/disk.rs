//! Finalize-time artifact quota exhaustion through the staged production daemon.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
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
            OsStr::new("qualify-disk-blob-finalize-stage"),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("inject.stdout"),
        &runtime.root.join("inject.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd artifact quota injection failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let checkpoint = one_line(&output.stdout, "artifact quota injection")?;
    let stage = parse_stage(&checkpoint)?;
    if stage.filler_sha256 == stage.rejected_sha256
        || stage.quota_bytes == 0
        || stage.temporary_files != 0
        || stage.object_files != 1
    {
        return Err("artifact quota checkpoint did not retain one exact admitted object".into());
    }
    inspect_layout(runtime, &stage.filler_sha256, &stage.rejected_sha256)?;
    Ok(InjectedCandidate {
        checkpoint,
        claim_fence: None,
        request_sha256: None,
        effect_path: None,
        effect_sha256: Some(stage.filler_sha256),
        effect_bytes: Some(stage.quota_bytes),
        artifact_sha256: Some(stage.rejected_sha256),
        artifact_bytes: Some(stage.quota_bytes),
        snapshot: None,
        lease: None,
        patch: None,
        gate: None,
        promotion: None,
        projection: None,
        fault_process_exit: "artifact-quota-exceeded".to_owned(),
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
            OsStr::new("qualify-disk-blob-finalize-recover"),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("recover.stdout"),
        &runtime.root.join("recover.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd artifact quota recovery failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let observation = one_line(&output.stdout, "artifact quota recovery")?;
    let recovered = parse_recovery(&observation)?;
    if recovered.filler_sha256 != stage.filler_sha256
        || recovered.rejected_sha256 != stage.rejected_sha256
        || recovered.quota_bytes != stage.quota_bytes
        || recovered.used_bytes != recovered.quota_bytes
        || !recovered.journal_verified
        || recovered.temporary_files != 0
        || recovered.object_files != 1
    {
        return Err("artifact quota recovery differs from the injected rollback".into());
    }
    inspect_layout(runtime, &recovered.filler_sha256, &recovered.rejected_sha256)?;
    let journal = runtime.state.join("peritus.sqlite3");
    let journal_metadata = fs::symlink_metadata(&journal)?;
    if !journal_metadata.file_type().is_file() || journal_metadata.len() == 0 {
        return Err("artifact quota recovery journal is missing".into());
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
        journal_bytes: journal_metadata.len(),
        effect_sha256: Some(recovered.filler_sha256),
        effect_bytes: Some(recovered.used_bytes),
        artifact_sha256: Some(recovered.rejected_sha256),
        artifact_bytes: Some(recovered.quota_bytes),
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
    filler_sha256: String,
    rejected_sha256: String,
    quota_bytes: u64,
    temporary_files: u64,
    object_files: u64,
}

struct RecoveryObservation {
    filler_sha256: String,
    rejected_sha256: String,
    quota_bytes: u64,
    used_bytes: u64,
    journal_verified: bool,
    temporary_files: u64,
    object_files: u64,
}

fn parse_stage(line: &str) -> Result<StageObservation, Box<dyn std::error::Error>> {
    let values = fields(line, "peritus-qualification disk-blob-finalize-stage ", 5)?;
    Ok(StageObservation {
        filler_sha256: sha256_field(values[0], "filler_sha256")?,
        rejected_sha256: sha256_field(values[1], "rejected_sha256")?,
        quota_bytes: number_field(values[2], "quota_bytes")?,
        temporary_files: number_field(values[3], "temporary_files")?,
        object_files: number_field(values[4], "object_files")?,
    })
}

fn parse_recovery(line: &str) -> Result<RecoveryObservation, Box<dyn std::error::Error>> {
    let values = fields(line, "peritus-qualification disk-blob-finalize-recover ", 7)?;
    Ok(RecoveryObservation {
        filler_sha256: sha256_field(values[0], "filler_sha256")?,
        rejected_sha256: sha256_field(values[1], "rejected_sha256")?,
        quota_bytes: number_field(values[2], "quota_bytes")?,
        used_bytes: number_field(values[3], "used_bytes")?,
        journal_verified: boolean_field(values[4], "journal_verified")?,
        temporary_files: number_field(values[5], "temporary_files")?,
        object_files: number_field(values[6], "object_files")?,
    })
}

fn inspect_layout(
    runtime: &RuntimePaths,
    filler_sha256: &str,
    rejected_sha256: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let objects = runtime.state.join("artifacts").join("objects").join("sha256");
    let filler = digest_path(&objects, filler_sha256);
    let rejected = digest_path(&objects, rejected_sha256);
    let filler_metadata = fs::symlink_metadata(&filler)?;
    if !filler_metadata.file_type().is_file() || rejected.exists() {
        return Err("controller observed an invalid artifact quota layout".into());
    }
    Ok(())
}

fn digest_path(root: &Path, digest: &str) -> PathBuf {
    root.join(&digest[..2]).join(digest)
}

fn fields<'a>(
    line: &'a str,
    prefix: &str,
    count: usize,
) -> Result<Vec<&'a str>, Box<dyn std::error::Error>> {
    let values = line
        .strip_prefix(prefix)
        .ok_or("staged peritusd returned an unknown artifact quota observation")?
        .split_ascii_whitespace()
        .collect::<Vec<_>>();
    if values.len() != count {
        return Err("artifact quota observation has the wrong field count".into());
    }
    Ok(values)
}

fn sha256_field(field: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = value_field(field, name)?;
    if !lower_sha256(value) {
        return Err(format!("artifact quota field {name} is not canonical SHA-256").into());
    }
    Ok(value.to_owned())
}

fn number_field<T: std::str::FromStr>(
    field: &str,
    name: &str,
) -> Result<T, Box<dyn std::error::Error>>
where
    T::Err: std::error::Error + 'static,
{
    value_field(field, name)?.parse::<T>().map_err(Into::into)
}

fn boolean_field(field: &str, name: &str) -> Result<bool, Box<dyn std::error::Error>> {
    match value_field(field, name)? {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("artifact quota field {name} is not boolean").into()),
    }
}

fn value_field<'a>(field: &'a str, name: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    let (observed, value) = field.split_once('=').ok_or("malformed artifact quota field")?;
    if observed != name || value.is_empty() {
        return Err(format!("expected artifact quota field {name}, found {observed}").into());
    }
    Ok(value)
}
