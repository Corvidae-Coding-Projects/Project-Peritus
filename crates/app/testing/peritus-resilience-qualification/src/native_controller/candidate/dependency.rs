//! Real provider, product-tool, and worker failure routes over durable scheduler state.

use std::ffi::OsStr;
use std::fs;
use std::time::Instant;

use crate::digest;
use crate::native_controller::args::{ControllerPaths, lower_sha256};
use crate::native_controller::request::ScenarioRoute;

use super::process::{bounded_command, one_line};
use super::{DependencyObservation, InjectedCandidate, RecoveredCandidate, RuntimePaths};

pub(super) fn inject(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
    route: ScenarioRoute,
    retry_limit: Option<u16>,
) -> Result<InjectedCandidate, Box<dyn std::error::Error>> {
    let (dependency, fault) = route_identity(route)?;
    let retry_limit = retry_limit.ok_or("dependency route has no retry limit")?;
    let retry_limit_text = retry_limit.to_string();
    let output = bounded_command(
        &paths.candidate,
        [
            OsStr::new("qualify-dependency-stage"),
            OsStr::new(dependency),
            OsStr::new(fault),
            OsStr::new("--attempts"),
            OsStr::new(&retry_limit_text),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("inject.stdout"),
        &runtime.root.join("inject.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd dependency fault failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let checkpoint = one_line(&output.stdout, "dependency fault")?;
    let stage = parse_stage(&checkpoint)?;
    let expected_attempts = if route.is_retry_exhaustion() { retry_limit } else { 1 };
    let expected_events = stage_event_count(expected_attempts);
    let expected_exit = (dependency != "worker").then_some(17);
    if stage.dependency != dependency
        || stage.fault != fault
        || stage.attempts != expected_attempts
        || stage.committed_events != expected_events
        || stage.child_exit != expected_exit
        || (dependency == "tool") != (stage.receipt_bytes > 0)
    {
        return Err("dependency fault checkpoint differs from the requested route".into());
    }
    Ok(InjectedCandidate {
        checkpoint,
        claim_fence: None,
        request_sha256: None,
        effect_path: None,
        effect_sha256: Some(stage.effect_sha256),
        effect_bytes: Some(stage.receipt_bytes),
        artifact_sha256: Some(stage.state_sha256),
        artifact_bytes: None,
        snapshot: None,
        lease: None,
        patch: None,
        gate: None,
        promotion: None,
        projection: None,
        fault_process_exit: expected_exit.map_or_else(
            || "worker-supervisor-aborted".to_owned(),
            |code| format!("dependency-child-exit={code}"),
        ),
    })
}

pub(super) fn recover(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
    injected: &InjectedCandidate,
    route: ScenarioRoute,
    retry_limit: Option<u16>,
) -> Result<RecoveredCandidate, Box<dyn std::error::Error>> {
    let started = Instant::now();
    let (dependency, fault) = route_identity(route)?;
    let retry_limit = retry_limit.ok_or("dependency route has no retry limit")?;
    let retry_limit_text = retry_limit.to_string();
    let output = bounded_command(
        &paths.candidate,
        [
            OsStr::new("qualify-dependency-recover"),
            OsStr::new(dependency),
            OsStr::new(fault),
            OsStr::new("--attempts"),
            OsStr::new(&retry_limit_text),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("recover.stdout"),
        &runtime.root.join("recover.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd dependency recovery failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let observation = one_line(&output.stdout, "dependency recovery")?;
    let recovered = parse_recovery(&observation)?;
    let stage = parse_stage(&injected.checkpoint)?;
    let expected_attempts = if route.is_retry_exhaustion() { retry_limit } else { 1 };
    let expected_events = if route.is_retry_exhaustion() {
        stage_event_count(expected_attempts)
    } else {
        stage_event_count(expected_attempts) + 1
    };
    let state_relation = if route.is_retry_exhaustion() {
        recovered.state_sha256 == stage.state_sha256
    } else {
        recovered.state_sha256 != stage.state_sha256
    };
    if recovered.dependency != dependency
        || recovered.fault != fault
        || recovered.attempts != expected_attempts
        || recovered.attempts != stage.attempts
        || recovered.committed_events != expected_events
        || recovered.aggregate_heads != 1
        || recovered.state
            != if route.is_retry_exhaustion() {
                RecoveredWorkState::Exhausted
            } else {
                RecoveredWorkState::Requeued
            }
        || !recovered.ownership_reconciled
        || !recovered.journal_verified
        || !state_relation
    {
        return Err("dependency recovery did not reconcile the exact scheduler state".into());
    }
    let journal = runtime.state.join("peritus.sqlite3");
    let metadata = fs::symlink_metadata(&journal)?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err("dependency scheduler journal is missing after recovery".into());
    }
    let external_effects = if dependency == "worker" { 0 } else { u64::from(stage.attempts) };
    Ok(RecoveredCandidate {
        observation,
        destination_reconciled: true,
        external_effects,
        duplicate_effects: 0,
        exact_fence_acknowledged: false,
        pending_claims: 0,
        committed_events: Some(recovered.committed_events),
        aggregate_heads: Some(recovered.aggregate_heads),
        journal_sha256: digest::hex(digest::file(&journal)?),
        journal_bytes: metadata.len(),
        effect_sha256: Some(stage.effect_sha256.clone()),
        effect_bytes: Some(stage.receipt_bytes),
        artifact_sha256: Some(recovered.state_sha256.clone()),
        artifact_bytes: None,
        snapshot: None,
        lease: None,
        patch: None,
        gate: None,
        promotion: None,
        projection: None,
        dependency: Some(DependencyObservation {
            dependency: dependency.to_owned(),
            fault: fault.to_owned(),
            attempts: recovered.attempts,
            committed_events: recovered.committed_events,
            aggregate_heads: recovered.aggregate_heads,
            state_sha256: recovered.state_sha256,
            effect_sha256: stage.effect_sha256,
            receipt_bytes: stage.receipt_bytes,
            exhausted: recovered.state == RecoveredWorkState::Exhausted,
            ownership_reconciled: recovered.ownership_reconciled,
        }),
        elapsed_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

fn route_identity(
    route: ScenarioRoute,
) -> Result<(&'static str, &'static str), Box<dyn std::error::Error>> {
    match (route.dependency(), route.dependency_fault()) {
        (Some(dependency), Some(fault)) => Ok((dependency, fault)),
        _ => Err("non-dependency scenario reached dependency candidate route".into()),
    }
}

fn stage_event_count(attempts: u16) -> u64 {
    u64::from(attempts) * 4 + 2
}

struct StageObservation {
    dependency: String,
    fault: String,
    state_sha256: String,
    effect_sha256: String,
    attempts: u16,
    committed_events: u64,
    receipt_bytes: u64,
    child_exit: Option<i32>,
}

fn parse_stage(line: &str) -> Result<StageObservation, Box<dyn std::error::Error>> {
    let values = fields(line, "peritus-qualification dependency-stage ", 8)?;
    Ok(StageObservation {
        dependency: text_field(values[0], "dependency")?.to_owned(),
        fault: text_field(values[1], "fault")?.to_owned(),
        state_sha256: sha256_field(values[2], "state_sha256")?,
        effect_sha256: sha256_field(values[3], "effect_sha256")?,
        attempts: number_field(values[4], "attempts")?,
        committed_events: number_field(values[5], "committed_events")?,
        receipt_bytes: number_field(values[6], "receipt_bytes")?,
        child_exit: optional_i32_field(values[7], "child_exit")?,
    })
}

struct RecoveryObservation {
    dependency: String,
    fault: String,
    state_sha256: String,
    attempts: u16,
    committed_events: u64,
    aggregate_heads: u64,
    state: RecoveredWorkState,
    ownership_reconciled: bool,
    journal_verified: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RecoveredWorkState {
    Requeued,
    Exhausted,
}

fn parse_recovery(line: &str) -> Result<RecoveryObservation, Box<dyn std::error::Error>> {
    let values = fields(line, "peritus-qualification dependency-recover ", 10)?;
    let state = match (
        boolean_field(values[6], "retry_pending")?,
        boolean_field(values[7], "exhausted")?,
    ) {
        (false, false) => RecoveredWorkState::Requeued,
        (false, true) => RecoveredWorkState::Exhausted,
        _ => return Err("dependency recovery returned an invalid terminal state".into()),
    };
    Ok(RecoveryObservation {
        dependency: text_field(values[0], "dependency")?.to_owned(),
        fault: text_field(values[1], "fault")?.to_owned(),
        state_sha256: sha256_field(values[2], "state_sha256")?,
        attempts: number_field(values[3], "attempts")?,
        committed_events: number_field(values[4], "committed_events")?,
        aggregate_heads: number_field(values[5], "aggregate_heads")?,
        state,
        ownership_reconciled: boolean_field(values[8], "ownership_reconciled")?,
        journal_verified: boolean_field(values[9], "journal_verified")?,
    })
}

fn fields<'a>(
    line: &'a str,
    prefix: &str,
    count: usize,
) -> Result<Vec<&'a str>, Box<dyn std::error::Error>> {
    let values = line
        .strip_prefix(prefix)
        .ok_or("staged peritusd returned an unknown dependency observation")?
        .split_ascii_whitespace()
        .collect::<Vec<_>>();
    if values.len() != count {
        return Err("dependency observation has the wrong field count".into());
    }
    Ok(values)
}

fn sha256_field(field: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = text_field(field, name)?;
    if !lower_sha256(value) {
        return Err(format!("dependency field {name} is not canonical SHA-256").into());
    }
    Ok(value.to_owned())
}

fn boolean_field(field: &str, name: &str) -> Result<bool, Box<dyn std::error::Error>> {
    match text_field(field, name)? {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("dependency field {name} is not boolean").into()),
    }
}

fn number_field<T: std::str::FromStr>(
    field: &str,
    name: &str,
) -> Result<T, Box<dyn std::error::Error>>
where
    T::Err: std::error::Error + 'static,
{
    text_field(field, name)?.parse::<T>().map_err(Into::into)
}

fn optional_i32_field(field: &str, name: &str) -> Result<Option<i32>, Box<dyn std::error::Error>> {
    let value = text_field(field, name)?;
    if value == "none" { Ok(None) } else { value.parse::<i32>().map(Some).map_err(Into::into) }
}

fn text_field<'a>(field: &'a str, name: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    let (observed, value) = field.split_once('=').ok_or("malformed dependency field")?;
    if observed != name || value.is_empty() {
        return Err(format!("expected dependency field {name}, found {observed}").into());
    }
    Ok(value)
}
