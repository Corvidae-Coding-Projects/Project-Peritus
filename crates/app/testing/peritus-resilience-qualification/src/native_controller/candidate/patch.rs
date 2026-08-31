//! Staged-daemon effects for patch application on both sides of durable commit.

use std::ffi::OsStr;
use std::fs;
use std::time::Instant;

use crate::digest;

use super::process::{bounded_command, kill_after_checkpoint, one_line};
use super::{InjectedCandidate, PatchObservation, RecoveredCandidate, RuntimePaths};
use crate::native_controller::args::{ControllerPaths, lower_sha256};
use crate::native_controller::request::ScenarioRoute;

pub(super) fn inject(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
    route: ScenarioRoute,
) -> Result<InjectedCandidate, Box<dyn std::error::Error>> {
    let (command, prefix, count, committed) = route_parameters(route)?;
    let killed = kill_after_checkpoint(
        &paths.candidate,
        [OsStr::new(command), OsStr::new("--config"), runtime.config.as_os_str()],
        &runtime.root,
        &runtime.root.join("inject.stderr"),
    )?;
    let values = fields(&killed.line, prefix, count)?;
    let patch_sha256 = sha256_field(values[0], "patch_sha256")?;
    let target_sha256 = sha256_field(values[1], "target_sha256")?;
    let manifest_sha256 = if committed {
        if !boolean_field(values[3], "applied")? {
            return Err("patch after-commit checkpoint is not applied".into());
        }
        Some(sha256_field(values[2], "manifest_sha256")?)
    } else {
        None
    };
    let target = observe_target(runtime)?;
    if target.as_ref().map(|target| &target.sha256) != committed.then_some(&target_sha256) {
        return Err("controller observed patch target bytes unlike the commit checkpoint".into());
    }
    Ok(InjectedCandidate {
        checkpoint: killed.line,
        claim_fence: None,
        request_sha256: None,
        effect_path: target.as_ref().map(|target| target.path.to_string_lossy().into_owned()),
        effect_sha256: target.as_ref().map(|target| target.sha256.clone()),
        effect_bytes: target.as_ref().map(|target| target.bytes),
        artifact_sha256: None,
        artifact_bytes: None,
        snapshot: None,
        lease: None,
        patch: Some(PatchObservation {
            identity: patch_sha256,
            postimage: Some(target_sha256),
            receipt_manifest: manifest_sha256,
        }),
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
    let (stage_command, stage_prefix, _, committed) = route_parameters(route)?;
    let command = stage_command.replace("-stage", "-recover");
    let prefix = stage_prefix.replace("-stage ", "-recover ");
    let output = bounded_command(
        &paths.candidate,
        [OsStr::new(&command), OsStr::new("--config"), runtime.config.as_os_str()],
        &runtime.root,
        &runtime.root.join("recover.stdout"),
        &runtime.root.join("recover.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd patch recovery failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let output_line = one_line(&output.stdout, "patch recovery")?;
    let values = fields(&output_line, &prefix, 5)?;
    let recovered = PatchObservation {
        identity: sha256_field(values[0], "patch_sha256")?,
        postimage: optional_sha256(values[1], "target_sha256")?,
        receipt_manifest: None,
    };
    let injected_patch = injected.patch.as_ref().ok_or("patch checkpoint identity missing")?;
    let expected_target = committed
        .then(|| injected_patch.postimage.clone().expect("validated checkpoint target digest"));
    if recovered.identity != injected_patch.identity
        || recovered.postimage != expected_target
        || !boolean_field(values[2], "journal_verified")?
        || number_field(values[3], "target_files")? != u64::from(committed)
        || number_field(values[4], "pending_transactions")? != 0
    {
        return Err("staged peritusd patch recovery differs from the commit boundary".into());
    }
    let target = observe_target(runtime)?;
    if target.as_ref().map(|target| &target.sha256) != recovered.postimage.as_ref()
        || target.as_ref().map(|target| target.bytes) != injected.effect_bytes
    {
        return Err("controller observed changed patch target bytes after recovery".into());
    }
    let journal = runtime.state.join("peritus.sqlite3");
    let metadata = fs::symlink_metadata(&journal)?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err("patch recovery journal is missing or empty".into());
    }
    Ok(RecoveredCandidate {
        observation: output_line,
        destination_reconciled: true,
        external_effects: u64::from(committed),
        duplicate_effects: 0,
        exact_fence_acknowledged: false,
        pending_claims: 0,
        committed_events: Some(0),
        aggregate_heads: Some(0),
        journal_sha256: digest::hex(digest::file(&journal)?),
        journal_bytes: metadata.len(),
        effect_sha256: target.as_ref().map(|target| target.sha256.clone()),
        effect_bytes: target.as_ref().map(|target| target.bytes),
        artifact_sha256: None,
        artifact_bytes: None,
        snapshot: None,
        lease: None,
        patch: Some(recovered),
        gate: None,
        promotion: None,
        projection: None,
        dependency: None,
        elapsed_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

struct TargetObservation {
    path: std::path::PathBuf,
    sha256: String,
    bytes: u64,
}

fn observe_target(
    runtime: &RuntimePaths,
) -> Result<Option<TargetObservation>, Box<dyn std::error::Error>> {
    let target =
        runtime.state.join("patch-crash-qualification-v1").join("workspace").join("delivery.txt");
    match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(Some(TargetObservation {
            sha256: digest::hex(digest::file(&target)?),
            bytes: metadata.len(),
            path: fs::canonicalize(target)?,
        })),
        Ok(_) => Err("patch target is not a regular file".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn route_parameters(
    route: ScenarioRoute,
) -> Result<(&'static str, &'static str, usize, bool), Box<dyn std::error::Error>> {
    match route {
        ScenarioRoute::PatchBeforeDurableCommit => Ok((
            "qualify-patch-before-stage",
            "peritus-qualification patch-before-stage ",
            2,
            false,
        )),
        ScenarioRoute::PatchAfterDurableCommitBeforeAck => {
            Ok(("qualify-patch-after-stage", "peritus-qualification patch-after-stage ", 4, true))
        }
        _ => Err("patch controller received a non-patch route".into()),
    }
}

fn fields<'a>(
    line: &'a str,
    prefix: &str,
    count: usize,
) -> Result<Vec<&'a str>, Box<dyn std::error::Error>> {
    let fields = line
        .strip_prefix(prefix)
        .ok_or("staged peritusd returned an unknown patch observation")?
        .split_whitespace()
        .collect::<Vec<_>>();
    if fields.len() != count {
        return Err("patch observation has the wrong field count".into());
    }
    Ok(fields)
}

fn sha256_field(field: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = value_field(field, name)?;
    if !lower_sha256(value) {
        return Err(format!("patch field {name} is not canonical SHA-256").into());
    }
    Ok(value.to_owned())
}

fn optional_sha256(field: &str, name: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let value = value_field(field, name)?;
    if value == "none" { Ok(None) } else { sha256_field(field, name).map(Some) }
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
    if observed != name || value.is_empty() {
        return Err(format!("expected staged peritusd field {name}, found {observed}").into());
    }
    Ok(value)
}
