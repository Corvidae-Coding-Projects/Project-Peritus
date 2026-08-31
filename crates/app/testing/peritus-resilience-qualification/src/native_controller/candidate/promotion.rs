//! Staged-daemon effects for the complete F0 atomic promotion boundary.

use std::ffi::OsStr;
use std::fs;
use std::time::Instant;

use crate::digest;

use super::process::{bounded_command, kill_after_checkpoint, one_line};
use super::{
    InjectedCandidate, PromotionCheckpoint, PromotionObservation, RecoveredCandidate, RuntimePaths,
};
use crate::native_controller::args::{ControllerPaths, lower_sha256};
use crate::native_controller::request::ScenarioRoute;

const PREPARED_EVENTS: u64 = 14;
const COMMITTED_EVENTS: u64 = 16;
const AGGREGATE_HEADS: u64 = 4;

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
    let checkpoint = PromotionCheckpoint {
        proposal_sha256: sha256_field(values[0], "proposal_sha256")?,
        authorization_sha256: sha256_field(values[1], "authorization_sha256")?,
        campaign_before_sha256: sha256_field(values[2], "campaign_before_sha256")?,
        pointer_before_sha256: sha256_field(values[3], "pointer_before_sha256")?,
        campaign_after_sha256: sha256_field(values[4], "campaign_after_sha256")?,
        pointer_after_sha256: sha256_field(values[5], "pointer_after_sha256")?,
        approval_revision: committed
            .then(|| number_field(values[6], "approval_revision"))
            .transpose()?,
        first_position: committed.then(|| number_field(values[7], "first_position")).transpose()?,
        last_position: committed.then(|| number_field(values[8], "last_position")).transpose()?,
        committed,
    };
    if committed
        && (checkpoint.approval_revision != Some(2)
            || checkpoint.first_position != Some(15)
            || checkpoint.last_position != Some(COMMITTED_EVENTS)
            || !boolean_field(values[9], "committed")?)
    {
        return Err("promotion checkpoint does not contain the complete atomic receipt".into());
    }
    Ok(InjectedCandidate {
        checkpoint: killed.line,
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
        promotion: Some(checkpoint),
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
            "staged peritusd promotion recovery failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let output_line = one_line(&output.stdout, "promotion recovery")?;
    let values = fields(&output_line, &prefix, 9)?;
    let recovered = PromotionObservation {
        proposal_sha256: sha256_field(values[0], "proposal_sha256")?,
        authorization_sha256: optional_sha256(values[1], "authorization_sha256")?,
        campaign_sha256: sha256_field(values[2], "campaign_sha256")?,
        pointer_sha256: sha256_field(values[3], "pointer_sha256")?,
        approval_revision: number_field(values[4], "approval_revision")?,
        approval_position: number_field(values[5], "approval_position")?,
        committed_events: number_field(values[6], "committed_events")?,
        aggregate_heads: number_field(values[7], "aggregate_heads")?,
        committed: boolean_field(values[8], "committed")?,
    };
    verify_recovery(injected, &recovered, committed)?;
    let journal = runtime.state.join("peritus.sqlite3");
    let metadata = fs::symlink_metadata(&journal)?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err("promotion recovery journal is missing or empty".into());
    }
    Ok(RecoveredCandidate {
        observation: output_line,
        destination_reconciled: true,
        external_effects: 0,
        duplicate_effects: 0,
        exact_fence_acknowledged: false,
        pending_claims: 0,
        committed_events: Some(recovered.committed_events),
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
        promotion: Some(recovered),
        projection: None,
        dependency: None,
        elapsed_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

fn verify_recovery(
    injected: &InjectedCandidate,
    recovered: &PromotionObservation,
    committed: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let checkpoint = injected.promotion.as_ref().ok_or("promotion checkpoint identity missing")?;
    let expected_authorization = committed.then(|| checkpoint.authorization_sha256.clone());
    let expected_campaign = if committed {
        &checkpoint.campaign_after_sha256
    } else {
        &checkpoint.campaign_before_sha256
    };
    let expected_pointer = if committed {
        &checkpoint.pointer_after_sha256
    } else {
        &checkpoint.pointer_before_sha256
    };
    let expected_events = if committed { COMMITTED_EVENTS } else { PREPARED_EVENTS };
    let expected_revision = if committed { 2 } else { 1 };
    if checkpoint.committed != committed
        || recovered.proposal_sha256 != checkpoint.proposal_sha256
        || recovered.authorization_sha256 != expected_authorization
        || &recovered.campaign_sha256 != expected_campaign
        || &recovered.pointer_sha256 != expected_pointer
        || recovered.approval_revision != expected_revision
        || recovered.approval_position != expected_events
        || recovered.committed_events != expected_events
        || recovered.aggregate_heads != AGGREGATE_HEADS
        || recovered.committed != committed
    {
        return Err("staged peritusd promotion recovery crosses the atomic commit boundary".into());
    }
    Ok(())
}

fn route_parameters(
    route: ScenarioRoute,
) -> Result<(&'static str, &'static str, usize, bool), Box<dyn std::error::Error>> {
    match route {
        ScenarioRoute::PromotionBeforeDurableCommit => Ok((
            "qualify-promotion-before-stage",
            "peritus-qualification promotion-before-stage ",
            6,
            false,
        )),
        ScenarioRoute::PromotionAfterDurableCommitBeforeAck => Ok((
            "qualify-promotion-after-stage",
            "peritus-qualification promotion-after-stage ",
            10,
            true,
        )),
        _ => Err("promotion controller received a non-promotion route".into()),
    }
}

fn fields<'a>(
    line: &'a str,
    prefix: &str,
    count: usize,
) -> Result<Vec<&'a str>, Box<dyn std::error::Error>> {
    let fields = line
        .strip_prefix(prefix)
        .ok_or("staged peritusd returned an unknown promotion observation")?
        .split_whitespace()
        .collect::<Vec<_>>();
    if fields.len() != count {
        return Err("promotion observation has the wrong field count".into());
    }
    Ok(fields)
}

fn sha256_field(field: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = value_field(field, name)?;
    if !lower_sha256(value) {
        return Err(format!("promotion field {name} is not canonical SHA-256").into());
    }
    Ok(value.to_owned())
}

fn optional_sha256(field: &str, name: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
    if value_field(field, name)? == "none" { Ok(None) } else { sha256_field(field, name).map(Some) }
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
