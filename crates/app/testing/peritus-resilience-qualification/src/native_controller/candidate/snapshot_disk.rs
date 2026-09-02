//! Snapshot manifest quota exhaustion through the staged production daemon.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::digest;
use crate::native_controller::args::{ControllerPaths, lower_sha256};

use super::process::{bounded_command, one_line};
use super::{InjectedCandidate, RecoveredCandidate, RuntimePaths, SnapshotObservation};

const SNAPSHOT_ROOT: &str = "snapshot-crash-qualification-v1";
const ARTIFACT_DIRECTORY: &str = "snapshot-manifest-artifacts";

pub(super) fn inject(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
) -> Result<InjectedCandidate, Box<dyn std::error::Error>> {
    let output = bounded_command(
        &paths.candidate,
        [
            OsStr::new("qualify-disk-snapshot-commit-stage"),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("inject.stdout"),
        &runtime.root.join("inject.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd snapshot quota injection failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let checkpoint = one_line(&output.stdout, "snapshot quota injection")?;
    let stage = parse_stage(&checkpoint)?;
    if stage.quota_bytes != 4_096
        || stage.snapshot_refs != 0
        || stage.temporary_files != 0
        || stage.object_files != 1
    {
        return Err("snapshot quota checkpoint retained partial publication state".into());
    }
    independently_verify(runtime, &stage.reference, &stage.filler_sha256)?;
    Ok(InjectedCandidate {
        checkpoint,
        claim_fence: None,
        request_sha256: None,
        effect_path: None,
        effect_sha256: Some(stage.filler_sha256),
        effect_bytes: Some(stage.quota_bytes),
        artifact_sha256: Some(stage.manifest_sha256.clone()),
        artifact_bytes: Some(stage.quota_bytes),
        snapshot: Some(SnapshotObservation {
            commit: None,
            tree: stage.tree,
            reference: stage.reference,
            manifest_sha256: Some(stage.manifest_sha256),
        }),
        lease: None,
        patch: None,
        gate: None,
        promotion: None,
        projection: None,
        fault_process_exit: "snapshot-manifest-quota-exceeded".to_owned(),
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
            OsStr::new("qualify-disk-snapshot-commit-recover"),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("recover.stdout"),
        &runtime.root.join("recover.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd snapshot quota recovery failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let observation = one_line(&output.stdout, "snapshot quota recovery")?;
    let recovered = parse_recovery(&observation)?;
    let stage = parse_stage(&injected.checkpoint)?;
    if recovered.stage != stage
        || recovered.used_bytes != recovered.stage.quota_bytes
        || !recovered.journal_verified
        || recovered.stage.snapshot_refs != 0
        || recovered.stage.temporary_files != 0
        || recovered.stage.object_files != 1
    {
        return Err("snapshot quota recovery differs from compensated absence".into());
    }
    independently_verify(runtime, &recovered.stage.reference, &recovered.stage.filler_sha256)?;
    let journal = runtime.state.join("peritus.sqlite3");
    let journal_metadata = fs::symlink_metadata(&journal)?;
    if !journal_metadata.file_type().is_file() || journal_metadata.len() == 0 {
        return Err("snapshot quota recovery journal is missing or empty".into());
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
        effect_sha256: Some(recovered.stage.filler_sha256.clone()),
        effect_bytes: Some(recovered.used_bytes),
        artifact_sha256: Some(recovered.stage.manifest_sha256.clone()),
        artifact_bytes: Some(recovered.stage.quota_bytes),
        snapshot: Some(SnapshotObservation {
            commit: None,
            tree: recovered.stage.tree,
            reference: recovered.stage.reference,
            manifest_sha256: Some(recovered.stage.manifest_sha256),
        }),
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

#[derive(PartialEq)]
struct StageObservation {
    filler_sha256: String,
    tree: String,
    reference: String,
    manifest_sha256: String,
    quota_bytes: u64,
    snapshot_refs: u64,
    temporary_files: u64,
    object_files: u64,
}

struct RecoveryObservation {
    stage: StageObservation,
    used_bytes: u64,
    journal_verified: bool,
}

fn parse_stage(line: &str) -> Result<StageObservation, Box<dyn std::error::Error>> {
    let values = fields(line, "peritus-qualification disk-snapshot-commit-stage ", 8)?;
    Ok(StageObservation {
        filler_sha256: sha256_field(values[0], "filler_sha256")?,
        tree: object_field(values[1], "tree")?,
        reference: reference_field(values[2], "reference")?,
        manifest_sha256: sha256_field(values[3], "manifest_sha256")?,
        quota_bytes: number_field(values[4], "quota_bytes")?,
        snapshot_refs: number_field(values[5], "snapshot_refs")?,
        temporary_files: number_field(values[6], "temporary_files")?,
        object_files: number_field(values[7], "object_files")?,
    })
}

fn parse_recovery(line: &str) -> Result<RecoveryObservation, Box<dyn std::error::Error>> {
    let values = fields(line, "peritus-qualification disk-snapshot-commit-recover ", 10)?;
    Ok(RecoveryObservation {
        stage: StageObservation {
            filler_sha256: sha256_field(values[0], "filler_sha256")?,
            tree: object_field(values[1], "tree")?,
            reference: reference_field(values[2], "reference")?,
            manifest_sha256: sha256_field(values[3], "manifest_sha256")?,
            quota_bytes: number_field(values[4], "quota_bytes")?,
            snapshot_refs: number_field(values[7], "snapshot_refs")?,
            temporary_files: number_field(values[8], "temporary_files")?,
            object_files: number_field(values[9], "object_files")?,
        },
        used_bytes: number_field(values[5], "used_bytes")?,
        journal_verified: boolean_field(values[6], "journal_verified")?,
    })
}

fn independently_verify(
    runtime: &RuntimePaths,
    reference: &str,
    filler_sha256: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let git_dir = runtime.state.join(SNAPSHOT_ROOT).join("repository").join(".git");
    if git_dir.join(reference).exists() {
        return Err("controller observed a retained snapshot ref after quota compensation".into());
    }
    let artifacts = runtime.state.join(SNAPSHOT_ROOT).join(ARTIFACT_DIRECTORY);
    let filler =
        artifacts.join("objects").join("sha256").join(&filler_sha256[..2]).join(filler_sha256);
    if count_files(&artifacts.join("temporary"), false)? != 0
        || count_files(&artifacts.join("objects"), true)? != 1
        || digest::hex(digest::file(&filler)?) != filler_sha256
    {
        return Err("controller observed snapshot manifest residue after quota compensation".into());
    }
    Ok(())
}

fn count_files(path: &Path, recursive: bool) -> Result<u64, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(0);
    }
    let mut pending = vec![PathBuf::from(path)];
    let mut count = 0_u64;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if recursive && kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                count = count.checked_add(1).ok_or("snapshot artifact count overflowed")?;
            }
        }
    }
    Ok(count)
}

fn fields<'a>(
    line: &'a str,
    prefix: &str,
    count: usize,
) -> Result<Vec<&'a str>, Box<dyn std::error::Error>> {
    let values = line
        .strip_prefix(prefix)
        .ok_or("staged peritusd returned an unknown snapshot quota observation")?
        .split_ascii_whitespace()
        .collect::<Vec<_>>();
    if values.len() != count {
        return Err("snapshot quota observation has the wrong field count".into());
    }
    Ok(values)
}

fn object_field(field: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = value_field(field, name)?;
    if matches!(value.len(), 40 | 64)
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(value.to_owned())
    } else {
        Err(format!("snapshot quota field {name} is not an object ID").into())
    }
}

fn reference_field(field: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = value_field(field, name)?;
    if value.starts_with("refs/peritus/workspaces/")
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'/')
    {
        Ok(value.to_owned())
    } else {
        Err(format!("snapshot quota field {name} is not a canonical Peritus ref").into())
    }
}

fn sha256_field(field: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = value_field(field, name)?;
    if lower_sha256(value) {
        Ok(value.to_owned())
    } else {
        Err(format!("snapshot quota field {name} is not canonical SHA-256").into())
    }
}

fn number_field(field: &str, name: &str) -> Result<u64, Box<dyn std::error::Error>> {
    value_field(field, name)?.parse::<u64>().map_err(Into::into)
}

fn boolean_field(field: &str, name: &str) -> Result<bool, Box<dyn std::error::Error>> {
    match value_field(field, name)? {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("snapshot quota field {name} is not boolean").into()),
    }
}

fn value_field<'a>(field: &'a str, name: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    let (observed, value) = field.split_once('=').ok_or("malformed snapshot quota field")?;
    if observed != name || value.is_empty() {
        return Err(format!("expected snapshot quota field {name}, found {observed}").into());
    }
    Ok(value)
}
