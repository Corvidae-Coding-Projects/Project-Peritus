//! Staged-daemon effects for D1 gate persistence on both sides of durable commit.

use std::ffi::OsStr;
use std::fs;
use std::time::Instant;

use crate::digest;

use super::process::{bounded_command, kill_after_checkpoint, one_line};
use super::{GateObservation, InjectedCandidate, RecoveredCandidate, RuntimePaths};
use crate::native_controller::args::{ControllerPaths, lower_sha256};
use crate::native_controller::request::CommitRoute;

pub(super) fn inject(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
    route: CommitRoute,
) -> Result<InjectedCandidate, Box<dyn std::error::Error>> {
    let (command, prefix, count, committed) = route_parameters(route)?;
    let killed = kill_after_checkpoint(
        &paths.candidate,
        [OsStr::new(command), OsStr::new("--config"), runtime.config.as_os_str()],
        &runtime.root,
        &runtime.root.join("inject.stderr"),
    )?;
    let fields = fields(&killed.line, prefix, count)?;
    let request_sha256 = sha256_field(fields[0], "request_sha256")?;
    let plan_sha256 = sha256_field(fields[1], "plan_sha256")?;
    let expected_successor = sha256_field(fields[2], "successor_sha256")?;
    let observation = if committed {
        if !boolean_field(fields[6], "committed")? {
            return Err("gate after-commit checkpoint is not committed".into());
        }
        GateObservation {
            request_sha256: request_sha256.clone(),
            plan_sha256,
            successor_sha256: Some(expected_successor),
            checkpoint_sha256: Some(sha256_field(fields[3], "checkpoint_sha256")?),
            state_revision: Some(number_field(fields[4], "state_revision")?),
            producing_position: Some(number_field(fields[5], "producing_position")?),
        }
    } else {
        GateObservation {
            request_sha256: request_sha256.clone(),
            plan_sha256,
            successor_sha256: None,
            checkpoint_sha256: None,
            state_revision: None,
            producing_position: None,
        }
    };
    Ok(InjectedCandidate {
        checkpoint: killed.line,
        claim_fence: None,
        request_sha256: Some(request_sha256),
        effect_path: None,
        effect_sha256: None,
        effect_bytes: None,
        artifact_sha256: None,
        artifact_bytes: None,
        snapshot: None,
        lease: None,
        patch: None,
        gate: Some(observation),
        killed_exit: killed.status,
    })
}

pub(super) fn recover(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
    injected: &InjectedCandidate,
    route: CommitRoute,
) -> Result<RecoveredCandidate, Box<dyn std::error::Error>> {
    let started = Instant::now();
    let (command, prefix, _, committed) = route_parameters(route)?;
    let recovery_command = command.replace("-stage", "-recover");
    let recovery_prefix = prefix.replace("-stage ", "-recover ");
    let output = bounded_command(
        &paths.candidate,
        [OsStr::new(&recovery_command), OsStr::new("--config"), runtime.config.as_os_str()],
        &runtime.root,
        &runtime.root.join("recover.stdout"),
        &runtime.root.join("recover.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd gate recovery failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let output_line = one_line(&output.stdout, "gate recovery")?;
    let values = fields(&output_line, &recovery_prefix, 9)?;
    let recovered = GateObservation {
        request_sha256: sha256_field(values[0], "request_sha256")?,
        plan_sha256: sha256_field(values[1], "plan_sha256")?,
        state_revision: optional_number(values[5], "state_revision")?,
        successor_sha256: optional_sha256(values[6], "successor_sha256")?,
        checkpoint_sha256: optional_sha256(values[7], "checkpoint_sha256")?,
        producing_position: optional_number(values[8], "producing_position")?,
    };
    let injected_gate = injected.gate.as_ref().ok_or("gate checkpoint identity missing")?;
    let expected_count = u64::from(committed);
    if !boolean_field(values[2], "journal_verified")?
        || number_field(values[3], "committed_events")? != expected_count
        || number_field(values[4], "aggregate_heads")? != expected_count
        || recovered.request_sha256 != injected_gate.request_sha256
        || recovered.plan_sha256 != injected_gate.plan_sha256
        || recovered.successor_sha256 != injected_gate.successor_sha256
        || recovered.checkpoint_sha256 != injected_gate.checkpoint_sha256
        || recovered.state_revision != injected_gate.state_revision
        || recovered.producing_position != injected_gate.producing_position
    {
        return Err("staged peritusd gate recovery differs from the commit boundary".into());
    }
    let journal = runtime.state.join("peritus.sqlite3");
    let metadata = fs::symlink_metadata(&journal)?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err("gate recovery journal is missing or empty".into());
    }
    Ok(RecoveredCandidate {
        observation: output_line,
        destination_reconciled: true,
        external_effects: 0,
        duplicate_effects: 0,
        exact_fence_acknowledged: false,
        pending_claims: 0,
        committed_events: Some(expected_count),
        aggregate_heads: Some(expected_count),
        journal_sha256: digest::hex(digest::file(&journal)?),
        journal_bytes: metadata.len(),
        effect_sha256: None,
        effect_bytes: None,
        artifact_sha256: None,
        artifact_bytes: None,
        snapshot: None,
        lease: None,
        patch: None,
        gate: Some(recovered),
        elapsed_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

fn route_parameters(
    route: CommitRoute,
) -> Result<(&'static str, &'static str, usize, bool), Box<dyn std::error::Error>> {
    match route {
        CommitRoute::GateBeforeDurableCommit => {
            Ok(("qualify-gate-before-stage", "peritus-qualification gate-before-stage ", 3, false))
        }
        CommitRoute::GateAfterDurableCommitBeforeAck => {
            Ok(("qualify-gate-after-stage", "peritus-qualification gate-after-stage ", 7, true))
        }
        _ => Err("gate controller received a non-gate route".into()),
    }
}

fn fields<'a>(
    line: &'a str,
    prefix: &str,
    count: usize,
) -> Result<Vec<&'a str>, Box<dyn std::error::Error>> {
    let fields = line
        .strip_prefix(prefix)
        .ok_or("staged peritusd returned an unknown gate observation")?
        .split_whitespace()
        .collect::<Vec<_>>();
    if fields.len() != count {
        return Err("gate observation has the wrong field count".into());
    }
    Ok(fields)
}

fn sha256_field(field: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = value_field(field, name)?;
    if !lower_sha256(value) {
        return Err(format!("gate field {name} is not canonical SHA-256").into());
    }
    Ok(value.to_owned())
}

fn optional_sha256(field: &str, name: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let value = value_field(field, name)?;
    if value == "none" { Ok(None) } else { sha256_field(field, name).map(Some) }
}

fn optional_number(field: &str, name: &str) -> Result<Option<u64>, Box<dyn std::error::Error>> {
    let value = value_field(field, name)?;
    if value == "none" { Ok(None) } else { value.parse::<u64>().map(Some).map_err(Into::into) }
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
