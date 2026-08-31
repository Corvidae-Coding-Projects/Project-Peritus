//! Recovery after the durable outbox effect but before fence acknowledgement.

use std::ffi::OsStr;
use std::fs;
use std::time::Instant;

use crate::digest;

use super::process::{bounded_command, kill_after_checkpoint, one_line};
use super::{InjectedCandidate, RecoveredCandidate, RuntimePaths};
use crate::native_controller::args::ControllerPaths;

const EFFECT_DIRECTORY: &str = "outbox-crash-qualification-v1";

pub(super) fn inject(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
) -> Result<InjectedCandidate, Box<dyn std::error::Error>> {
    let killed = kill_after_checkpoint(
        &paths.candidate,
        [OsStr::new("qualify-outbox-stage"), OsStr::new("--config"), runtime.config.as_os_str()],
        &runtime.root,
        &runtime.root.join("inject.stderr"),
    )?;
    let checkpoint = killed.line;
    let (effect_path, claim_fence) = parse_checkpoint(&checkpoint)?;
    let effect = fs::canonicalize(effect_path)?;
    let effect_root = fs::canonicalize(runtime.state.join(EFFECT_DIRECTORY))?;
    if !effect.starts_with(&effect_root) || !fs::symlink_metadata(&effect)?.file_type().is_file() {
        return Err("staged peritusd checkpoint names an invalid effect path".into());
    }
    let effect_bytes = fs::metadata(&effect)?.len();
    let effect_sha256 = digest::hex(digest::file(&effect)?);
    Ok(InjectedCandidate {
        checkpoint,
        claim_fence: Some(claim_fence),
        request_sha256: None,
        effect_path: Some(effect.to_string_lossy().into_owned()),
        effect_sha256: Some(effect_sha256),
        effect_bytes: Some(effect_bytes),
        artifact_sha256: None,
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
) -> Result<RecoveredCandidate, Box<dyn std::error::Error>> {
    let started = Instant::now();
    let output = bounded_command(
        &paths.candidate,
        [OsStr::new("qualify-outbox-recover"), OsStr::new("--config"), runtime.config.as_os_str()],
        &runtime.root,
        &runtime.root.join("recover.stdout"),
        &runtime.root.join("recover.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd recovery failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let observation = one_line(&output.stdout, "outbox crash recovery")?;
    let recovered = parse_recovery(&observation)?;
    verify_recovery(&recovered)?;
    let (effect, effect_sha256, effect_bytes) = verify_effect(runtime, injected)?;
    verify_only_effect(runtime, &effect)?;
    let journal = runtime.state.join("peritus.sqlite3");
    let journal_metadata = fs::symlink_metadata(&journal)?;
    if !journal_metadata.file_type().is_file() || journal_metadata.len() == 0 {
        return Err("recovered daemon journal is missing or empty".into());
    }
    Ok(RecoveredCandidate {
        observation,
        destination_reconciled: recovered.destination_reconciled,
        external_effects: recovered.external_effects,
        duplicate_effects: recovered.duplicate_effects,
        exact_fence_acknowledged: recovered.exact_fence_acknowledged,
        pending_claims: recovered.pending_claims,
        committed_events: None,
        aggregate_heads: None,
        journal_sha256: digest::hex(digest::file(&journal)?),
        journal_bytes: journal_metadata.len(),
        effect_sha256: Some(effect_sha256),
        effect_bytes: Some(effect_bytes),
        artifact_sha256: None,
        artifact_bytes: None,
        snapshot: None,
        lease: None,
        patch: None,
        gate: None,
        promotion: None,
        projection: None,
        elapsed_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

fn verify_effect(
    runtime: &RuntimePaths,
    injected: &InjectedCandidate,
) -> Result<(std::path::PathBuf, String, u64), Box<dyn std::error::Error>> {
    let injected_effect = injected
        .effect_path
        .as_deref()
        .ok_or("journal after-commit checkpoint omitted its effect path")?;
    let effect = fs::canonicalize(injected_effect)?;
    let effect_sha256 = digest::hex(digest::file(&effect)?);
    let effect_bytes = fs::metadata(&effect)?.len();
    if Some(&effect_sha256) != injected.effect_sha256.as_ref()
        || Some(effect_bytes) != injected.effect_bytes
        || !effect.starts_with(fs::canonicalize(runtime.state.join(EFFECT_DIRECTORY))?)
    {
        return Err("recovery changed or replaced the identity-bearing outbox effect".into());
    }
    Ok((effect, effect_sha256, effect_bytes))
}

fn verify_only_effect(
    runtime: &RuntimePaths,
    effect: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let entries =
        fs::read_dir(runtime.state.join(EFFECT_DIRECTORY))?.collect::<Result<Vec<_>, _>>()?;
    if entries.len() != 1
        || !entries[0].file_type()?.is_file()
        || fs::canonicalize(entries[0].path())? != effect
    {
        return Err("recovery left a duplicate or non-file outbox effect".into());
    }
    Ok(())
}

fn parse_checkpoint(line: &str) -> Result<(&str, u64), Box<dyn std::error::Error>> {
    let value = line
        .strip_prefix("peritus-qualification outbox-stage effect_path=")
        .ok_or("staged peritusd returned an unknown crash checkpoint")?;
    let (path, fence) = value
        .rsplit_once(" claim_fence=")
        .ok_or("staged peritusd omitted the crash checkpoint fence")?;
    let fence = fence.parse::<u64>()?;
    if path.is_empty() || fence == 0 {
        return Err("staged peritusd returned an invalid crash checkpoint".into());
    }
    Ok((path, fence))
}

struct ParsedRecovery {
    destination_reconciled: bool,
    external_effects: u64,
    duplicate_effects: u64,
    exact_fence_acknowledged: bool,
    pending_claims: u64,
}

fn verify_recovery(recovered: &ParsedRecovery) -> Result<(), Box<dyn std::error::Error>> {
    if !recovered.destination_reconciled
        || recovered.external_effects != 1
        || recovered.duplicate_effects != 0
        || !recovered.exact_fence_acknowledged
        || recovered.pending_claims != 0
    {
        return Err("staged peritusd did not reconcile and settle the exact outbox effect".into());
    }
    Ok(())
}

fn parse_recovery(line: &str) -> Result<ParsedRecovery, Box<dyn std::error::Error>> {
    let fields = line
        .strip_prefix("peritus-qualification outbox-recover ")
        .ok_or("staged peritusd returned an unknown recovery observation")?
        .split_whitespace()
        .collect::<Vec<_>>();
    if fields.len() != 5 {
        return Err("staged peritusd recovery observation has the wrong field count".into());
    }
    Ok(ParsedRecovery {
        destination_reconciled: boolean_field(fields[0], "destination_reconciled")?,
        external_effects: number_field(fields[1], "external_effects")?,
        duplicate_effects: number_field(fields[2], "duplicate_effects")?,
        exact_fence_acknowledged: boolean_field(fields[3], "exact_fence_acknowledged")?,
        pending_claims: number_field(fields[4], "pending_claims")?,
    })
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
    if observed != name {
        return Err(format!("expected staged peritusd field {name}, found {observed}").into());
    }
    Ok(value)
}
