//! Actual staged-daemon kill and fresh-process E0/C0 replay qualification.

use std::ffi::OsStr;
use std::fs;
use std::time::Instant;

use crate::digest;
use crate::native_controller::args::{ControllerPaths, lower_sha256};
use crate::native_controller::request::ScenarioRoute;

use super::process::{bounded_command, kill_after_checkpoint, one_line};
use super::{
    DaemonLifecycleCheckpointTruth, DaemonLifecycleObservation, DaemonLifecycleOwnership,
    DaemonLifecycleVerification, InjectedCandidate, RecoveredCandidate, RuntimePaths,
};

pub(super) fn inject(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
    route: ScenarioRoute,
) -> Result<InjectedCandidate, Box<dyn std::error::Error>> {
    let phase = route.daemon_phase().ok_or("daemon lifecycle route has no phase")?;
    let killed = kill_after_checkpoint(
        &paths.candidate,
        [
            OsStr::new("qualify-daemon-lifecycle-stage"),
            OsStr::new(phase.code()),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("inject.stderr"),
    )?;
    let stage = parse_stage(&killed.line)?;
    if stage.phase != phase.code()
        || stage.committed_events == 0
        || stage.state_sha256.len() != 64
        || stage.run_id.len() != 32
    {
        return Err("daemon lifecycle checkpoint differs from the requested phase".into());
    }
    Ok(InjectedCandidate {
        checkpoint: killed.line,
        claim_fence: None,
        request_sha256: None,
        effect_path: None,
        effect_sha256: None,
        effect_bytes: None,
        artifact_sha256: Some(stage.state_sha256),
        artifact_bytes: None,
        snapshot: None,
        lease: None,
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
    let phase = route.daemon_phase().ok_or("daemon lifecycle route has no phase")?;
    let output = bounded_command(
        &paths.candidate,
        [
            OsStr::new("qualify-daemon-lifecycle-recover"),
            OsStr::new(phase.code()),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("recover.stdout"),
        &runtime.root.join("recover.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd lifecycle replay failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let observation = one_line(&output.stdout, "daemon lifecycle recovery")?;
    let stage = parse_stage(&injected.checkpoint)?;
    let recovered = parse_recovery(&observation)?;
    if recovered.phase != phase.code()
        || recovered.run_id != stage.run_id
        || recovered.state_sha256 != stage.state_sha256
        || recovered.committed_events != stage.committed_events
        || recovered.ownership.active_children != stage.active_children
        || recovered.aggregate_heads != 1
        || !recovered.verification.replay_exact
        || !recovered.journal_verified
        || !recovered.verification.ownership_reconciled
    {
        return Err("fresh daemon did not replay the exact lifecycle checkpoint".into());
    }
    let journal = runtime.state.join("peritus.sqlite3");
    let metadata = fs::symlink_metadata(&journal)?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err("daemon lifecycle journal is missing after replay".into());
    }
    Ok(RecoveredCandidate {
        observation,
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
        artifact_sha256: Some(recovered.state_sha256.clone()),
        artifact_bytes: None,
        snapshot: None,
        lease: None,
        patch: None,
        gate: None,
        promotion: None,
        projection: None,
        dependency: None,
        lifecycle: Some(DaemonLifecycleObservation {
            phase: recovered.phase,
            state_sha256: recovered.state_sha256,
            committed_events: recovered.committed_events,
            aggregate_heads: recovered.aggregate_heads,
            ownership: recovered.ownership,
            checkpoint: recovered.checkpoint,
            verification: recovered.verification,
        }),
        elapsed_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

struct StageObservation {
    phase: String,
    run_id: String,
    state_sha256: String,
    committed_events: u64,
    active_children: u16,
}

fn parse_stage(line: &str) -> Result<StageObservation, Box<dyn std::error::Error>> {
    let fields = values(line, "peritus-qualification daemon-lifecycle-stage ", 5)?;
    let run_id = field(fields[1], "run_id")?.to_owned();
    let state_sha256 = field(fields[2], "state_sha256")?.to_owned();
    if !lower_hex(&run_id, 32) || !lower_sha256(&state_sha256) {
        return Err("daemon lifecycle checkpoint identity is not canonical".into());
    }
    Ok(StageObservation {
        phase: field(fields[0], "phase")?.to_owned(),
        run_id,
        state_sha256,
        committed_events: number(fields[3], "committed_events")?,
        active_children: number(fields[4], "active_children")?,
    })
}

struct RecoveryObservation {
    phase: String,
    run_id: String,
    state_sha256: String,
    committed_events: u64,
    aggregate_heads: u64,
    ownership: DaemonLifecycleOwnership,
    checkpoint: DaemonLifecycleCheckpointTruth,
    verification: DaemonLifecycleVerification,
    journal_verified: bool,
}

fn parse_recovery(line: &str) -> Result<RecoveryObservation, Box<dyn std::error::Error>> {
    let fields = values(line, "peritus-qualification daemon-lifecycle-recover ", 13)?;
    let run_id = field(fields[1], "run_id")?.to_owned();
    let state_sha256 = field(fields[2], "state_sha256")?.to_owned();
    if !lower_hex(&run_id, 32) || !lower_sha256(&state_sha256) {
        return Err("daemon lifecycle replay identity is not canonical".into());
    }
    Ok(RecoveryObservation {
        phase: field(fields[0], "phase")?.to_owned(),
        run_id,
        state_sha256,
        committed_events: number(fields[3], "committed_events")?,
        aggregate_heads: number(fields[4], "aggregate_heads")?,
        ownership: DaemonLifecycleOwnership {
            active_children: number(fields[5], "active_children")?,
            pending_directive: boolean(fields[6], "pending_directive")?,
            open_handoff: boolean(fields[7], "open_handoff")?,
        },
        checkpoint: DaemonLifecycleCheckpointTruth {
            proposed_candidate: boolean(fields[8], "proposed_candidate")?,
            acceptance_certificate: boolean(fields[9], "acceptance_certificate")?,
        },
        verification: DaemonLifecycleVerification {
            replay_exact: boolean(fields[10], "replay_exact")?,
            ownership_reconciled: boolean(fields[12], "ownership_reconciled")?,
        },
        journal_verified: boolean(fields[11], "journal_verified")?,
    })
}

fn values<'a>(
    line: &'a str,
    prefix: &str,
    count: usize,
) -> Result<Vec<&'a str>, Box<dyn std::error::Error>> {
    let fields = line
        .strip_prefix(prefix)
        .ok_or("staged peritusd returned an unknown lifecycle observation")?
        .split_ascii_whitespace()
        .collect::<Vec<_>>();
    if fields.len() != count {
        return Err("daemon lifecycle observation has the wrong field count".into());
    }
    Ok(fields)
}

fn field<'a>(value: &'a str, name: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    let (observed, text) = value.split_once('=').ok_or("malformed daemon lifecycle field")?;
    if observed == name {
        Ok(text)
    } else {
        Err(format!("expected daemon lifecycle field {name}, found {observed}").into())
    }
}

fn number<T: std::str::FromStr>(value: &str, name: &str) -> Result<T, Box<dyn std::error::Error>>
where
    T::Err: std::error::Error + 'static,
{
    Ok(field(value, name)?.parse::<T>()?)
}

fn boolean(value: &str, name: &str) -> Result<bool, Box<dyn std::error::Error>> {
    match field(value, name)? {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("daemon lifecycle field {name} is not boolean").into()),
    }
}

fn lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
