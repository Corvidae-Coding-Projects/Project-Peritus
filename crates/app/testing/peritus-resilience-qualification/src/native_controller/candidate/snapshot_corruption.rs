//! Controlled retained snapshot divergence and quarantine through the staged daemon.

use std::ffi::OsStr;
use std::fs;
use std::time::Instant;

use crate::digest;

use super::process::{bounded_command, one_line};
use super::{InjectedCandidate, RecoveredCandidate, RuntimePaths};
use crate::native_controller::args::{ControllerPaths, lower_sha256};

const SNAPSHOT_ROOT: &str = "snapshot-crash-qualification-v1";
const MANIFEST_FILE: &str = "snapshot-manifest-v1.bin";

pub(super) fn inject(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
) -> Result<InjectedCandidate, Box<dyn std::error::Error>> {
    let output = bounded_command(
        &paths.candidate,
        [
            OsStr::new("qualify-snapshot-corruption-stage"),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("inject.stdout"),
        &runtime.root.join("inject.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd snapshot corruption failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let checkpoint = one_line(&output.stdout, "snapshot corruption")?;
    let stage = parse_stage(&checkpoint)?;
    if stage.expected_commit == stage.divergent_commit || !stage.corruption_detected {
        return Err("snapshot corruption checkpoint did not contain exact divergence".into());
    }
    let manifest_bytes = independently_verify_stage(runtime, &stage)?;
    Ok(InjectedCandidate {
        checkpoint,
        claim_fence: None,
        request_sha256: None,
        effect_path: None,
        effect_sha256: None,
        effect_bytes: None,
        artifact_sha256: Some(stage.manifest_sha256),
        artifact_bytes: Some(manifest_bytes),
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
            OsStr::new("qualify-snapshot-corruption-recover"),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("recover.stdout"),
        &runtime.root.join("recover.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd snapshot containment failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let observation = one_line(&output.stdout, "snapshot corruption recovery")?;
    let recovered = parse_recovery(&observation)?;
    let stage = parse_stage(&injected.checkpoint)?;
    let quarantine_identity = (recovered.reference.as_str(), recovered.quarantined_commit.as_str());
    let injected_identity = (stage.reference.as_str(), stage.divergent_commit.as_str());
    if quarantine_identity != injected_identity
        || !recovered.journal_verified
        || !recovered.corruption_detected
        || recovered.mutation_admitted
    {
        return Err("staged daemon did not quarantine the divergent snapshot ref".into());
    }
    independently_verify_recovery(runtime, &recovered)?;
    let manifest = manifest_path(runtime);
    let manifest_bytes = fs::symlink_metadata(&manifest)?.len();
    if digest::hex(digest::file(&manifest)?) != stage.manifest_sha256 {
        return Err("snapshot manifest changed during quarantine".into());
    }
    let journal = runtime.state.join("peritus.sqlite3");
    let journal_metadata = fs::symlink_metadata(&journal)?;
    if !journal_metadata.file_type().is_file() || journal_metadata.len() == 0 {
        return Err("snapshot containment journal is missing or empty".into());
    }
    Ok(RecoveredCandidate {
        observation,
        destination_reconciled: true,
        external_effects: 1,
        duplicate_effects: 0,
        exact_fence_acknowledged: false,
        pending_claims: 0,
        committed_events: Some(0),
        aggregate_heads: Some(0),
        journal_sha256: digest::hex(digest::file(&journal)?),
        journal_bytes: journal_metadata.len(),
        effect_sha256: None,
        effect_bytes: None,
        artifact_sha256: Some(stage.manifest_sha256),
        artifact_bytes: Some(manifest_bytes),
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

fn independently_verify_stage(
    runtime: &RuntimePaths,
    stage: &StageObservation,
) -> Result<u64, Box<dyn std::error::Error>> {
    require_reference(&stage.reference, false)?;
    if loose_reference(runtime, &stage.reference)? != stage.divergent_commit {
        return Err("controller observed a different divergent snapshot commit".into());
    }
    let manifest = manifest_path(runtime);
    let metadata = fs::symlink_metadata(&manifest)?;
    if !metadata.file_type().is_file()
        || digest::hex(digest::file(&manifest)?) != stage.manifest_sha256
    {
        return Err("controller observed a different snapshot manifest".into());
    }
    Ok(metadata.len())
}

fn independently_verify_recovery(
    runtime: &RuntimePaths,
    recovered: &RecoveryObservation,
) -> Result<(), Box<dyn std::error::Error>> {
    require_reference(&recovered.reference, false)?;
    require_reference(&recovered.quarantine_reference, true)?;
    let active = git_dir(runtime).join(&recovered.reference);
    if active.exists()
        || loose_reference(runtime, &recovered.quarantine_reference)?
            != recovered.quarantined_commit
    {
        return Err("controller observed divergent snapshot state outside quarantine".into());
    }
    Ok(())
}

struct StageObservation {
    expected_commit: String,
    divergent_commit: String,
    reference: String,
    manifest_sha256: String,
    corruption_detected: bool,
}

fn parse_stage(line: &str) -> Result<StageObservation, Box<dyn std::error::Error>> {
    let fields = fields(line, "peritus-qualification snapshot-corruption-stage ", 5)?;
    Ok(StageObservation {
        expected_commit: object_field(fields[0], "expected_commit")?,
        divergent_commit: object_field(fields[1], "divergent_commit")?,
        reference: value_field(fields[2], "reference")?.to_owned(),
        manifest_sha256: sha256_field(fields[3], "manifest_sha256")?,
        corruption_detected: boolean_field(fields[4], "corruption_detected")?,
    })
}

struct RecoveryObservation {
    reference: String,
    quarantine_reference: String,
    quarantined_commit: String,
    journal_verified: bool,
    corruption_detected: bool,
    mutation_admitted: bool,
}

fn parse_recovery(line: &str) -> Result<RecoveryObservation, Box<dyn std::error::Error>> {
    let fields = fields(line, "peritus-qualification snapshot-corruption-recover ", 6)?;
    Ok(RecoveryObservation {
        reference: value_field(fields[0], "reference")?.to_owned(),
        quarantine_reference: value_field(fields[1], "quarantine_reference")?.to_owned(),
        quarantined_commit: object_field(fields[2], "quarantined_commit")?,
        journal_verified: boolean_field(fields[3], "journal_verified")?,
        corruption_detected: boolean_field(fields[4], "corruption_detected")?,
        mutation_admitted: boolean_field(fields[5], "mutation_admitted")?,
    })
}

fn fields<'a>(
    line: &'a str,
    prefix: &str,
    count: usize,
) -> Result<Vec<&'a str>, Box<dyn std::error::Error>> {
    let values = line
        .strip_prefix(prefix)
        .ok_or("staged peritusd returned an unknown snapshot corruption observation")?
        .split_ascii_whitespace()
        .collect::<Vec<_>>();
    if values.len() != count {
        return Err("snapshot corruption observation has the wrong field count".into());
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
        Err(format!("snapshot corruption field {name} is not an object ID").into())
    }
}

fn sha256_field(field: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = value_field(field, name)?;
    if lower_sha256(value) {
        Ok(value.to_owned())
    } else {
        Err(format!("snapshot corruption field {name} is not SHA-256").into())
    }
}

fn boolean_field(field: &str, name: &str) -> Result<bool, Box<dyn std::error::Error>> {
    match value_field(field, name)? {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("snapshot corruption field {name} is not boolean").into()),
    }
}

fn value_field<'a>(field: &'a str, name: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    let (observed, value) = field.split_once('=').ok_or("malformed snapshot corruption field")?;
    if observed != name || value.is_empty() {
        return Err(format!("expected snapshot corruption field {name}, found {observed}").into());
    }
    Ok(value)
}

fn require_reference(value: &str, quarantine: bool) -> Result<(), Box<dyn std::error::Error>> {
    let prefix =
        if quarantine { "refs/peritus/quarantine/workspaces/" } else { "refs/peritus/workspaces/" };
    if value.starts_with(prefix)
        && value.len() <= 320
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'/')
    {
        Ok(())
    } else {
        Err("snapshot corruption reference is not canonical".into())
    }
}

fn loose_reference(
    runtime: &RuntimePaths,
    reference: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let bytes = fs::read(git_dir(runtime).join(reference))?;
    let value = std::str::from_utf8(&bytes)?.trim_end_matches(['\r', '\n']);
    if value.is_empty() || value.contains(['\r', '\n']) {
        return Err("snapshot corruption reference value is malformed".into());
    }
    Ok(value.to_owned())
}

fn git_dir(runtime: &RuntimePaths) -> std::path::PathBuf {
    runtime.state.join(SNAPSHOT_ROOT).join("repository").join(".git")
}

fn manifest_path(runtime: &RuntimePaths) -> std::path::PathBuf {
    runtime.state.join(SNAPSHOT_ROOT).join(MANIFEST_FILE)
}
