//! Staged-daemon effects for lease persistence on both sides of durable commit.

use std::ffi::OsStr;
use std::fs;
use std::time::Instant;

use crate::digest;

use super::process::{bounded_command, kill_after_checkpoint, one_line};
use super::{InjectedCandidate, LeaseObservation, RecoveredCandidate, RuntimePaths};
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
    let fields = fields(&killed.line, prefix, count)?;
    let request_sha256 = sha256_field(fields[0], "request_sha256")?;
    let observation = if committed {
        if !boolean_field(fields[4], "committed")? {
            return Err("lease after-commit checkpoint is not committed".into());
        }
        LeaseObservation {
            request_sha256: request_sha256.clone(),
            state_revision: Some(number_field(fields[1], "state_revision")?),
            state_sha256: Some(sha256_field(fields[2], "state_sha256")?),
            producing_position: Some(number_field(fields[3], "producing_position")?),
        }
    } else {
        LeaseObservation {
            request_sha256: request_sha256.clone(),
            state_revision: None,
            state_sha256: None,
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
        lease: Some(observation),
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
            "staged peritusd lease recovery failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let output_line = one_line(&output.stdout, "lease recovery")?;
    let recovered_fields = fields(&output_line, &recovery_prefix, 7)?;
    let expected_count = u64::from(committed);
    let recovered = LeaseObservation {
        request_sha256: sha256_field(recovered_fields[0], "request_sha256")?,
        state_revision: optional_number(recovered_fields[4], "state_revision")?,
        state_sha256: optional_sha256(recovered_fields[5], "state_sha256")?,
        producing_position: optional_number(recovered_fields[6], "producing_position")?,
    };
    let injected_lease = injected.lease.as_ref().ok_or("lease checkpoint identity missing")?;
    if !boolean_field(recovered_fields[1], "journal_verified")?
        || number_field(recovered_fields[2], "committed_events")? != expected_count
        || number_field(recovered_fields[3], "aggregate_heads")? != expected_count
        || recovered.request_sha256 != injected_lease.request_sha256
        || recovered.state_revision != injected_lease.state_revision
        || recovered.state_sha256 != injected_lease.state_sha256
        || recovered.producing_position != injected_lease.producing_position
    {
        return Err("staged peritusd lease recovery differs from the commit boundary".into());
    }
    let journal = runtime.state.join("peritus.sqlite3");
    let metadata = fs::symlink_metadata(&journal)?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err("lease recovery journal is missing or empty".into());
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
        lease: Some(recovered),
        patch: None,
        gate: None,
        promotion: None,
        projection: None,
        dependency: None,
        elapsed_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

fn route_parameters(
    route: ScenarioRoute,
) -> Result<(&'static str, &'static str, usize, bool), Box<dyn std::error::Error>> {
    match route {
        ScenarioRoute::LeaseBeforeDurableCommit => Ok((
            "qualify-lease-before-stage",
            "peritus-qualification lease-before-stage ",
            1,
            false,
        )),
        ScenarioRoute::LeaseAfterDurableCommitBeforeAck => {
            Ok(("qualify-lease-after-stage", "peritus-qualification lease-after-stage ", 5, true))
        }
        _ => Err("lease controller received a non-lease route".into()),
    }
}

fn fields<'a>(
    line: &'a str,
    prefix: &str,
    count: usize,
) -> Result<Vec<&'a str>, Box<dyn std::error::Error>> {
    let fields = line
        .strip_prefix(prefix)
        .ok_or("staged peritusd returned an unknown lease observation")?
        .split_whitespace()
        .collect::<Vec<_>>();
    if fields.len() != count {
        return Err("lease observation has the wrong field count".into());
    }
    Ok(fields)
}

fn sha256_field(field: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = value_field(field, name)?;
    if !lower_sha256(value) {
        return Err(format!("lease field {name} is not canonical SHA-256").into());
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
