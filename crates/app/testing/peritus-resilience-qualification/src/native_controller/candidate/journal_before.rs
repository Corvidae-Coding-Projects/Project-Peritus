//! Staged-daemon effects for a crash before the first journal append is submitted.

use std::ffi::OsStr;
use std::fs;
use std::time::Instant;

use crate::digest;

use super::process::{bounded_command, kill_after_checkpoint, one_line};
use super::{InjectedCandidate, RecoveredCandidate, RuntimePaths};
use crate::native_controller::args::{ControllerPaths, lower_sha256};

const EFFECT_DIRECTORY: &str = "outbox-crash-qualification-v1";

pub(super) fn inject(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
) -> Result<InjectedCandidate, Box<dyn std::error::Error>> {
    let killed = kill_after_checkpoint(
        &paths.candidate,
        [
            OsStr::new("qualify-journal-before-stage"),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("inject.stderr"),
    )?;
    let request_sha256 = checkpoint_digest(&killed.line)?;
    Ok(InjectedCandidate {
        checkpoint: killed.line,
        claim_fence: None,
        request_sha256: Some(request_sha256),
        effect_path: None,
        effect_sha256: None,
        effect_bytes: None,
        killed_exit: killed.status,
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
            OsStr::new("qualify-journal-before-recover"),
            OsStr::new("--config"),
            runtime.config.as_os_str(),
        ],
        &runtime.root,
        &runtime.root.join("recover.stdout"),
        &runtime.root.join("recover.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd pre-commit recovery failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let observation = one_line(&output.stdout, "journal pre-commit recovery")?;
    let recovered = parse_recovery(&observation)?;
    if !recovered.journal_verified
        || recovered.committed_events != 0
        || recovered.aggregate_heads != 0
        || recovered.external_effects != 0
        || recovered.pending_claims != 0
        || Some(&recovered.request_sha256) != injected.request_sha256.as_ref()
    {
        return Err("staged peritusd retained state from an unsubmitted journal append".into());
    }
    let journal = runtime.state.join("peritus.sqlite3");
    let journal_metadata = fs::symlink_metadata(&journal)?;
    if !journal_metadata.file_type().is_file() || journal_metadata.len() == 0 {
        return Err("reopened pre-commit journal is missing or empty".into());
    }
    match fs::symlink_metadata(runtime.state.join(EFFECT_DIRECTORY)) {
        Ok(_) => return Err("pre-commit crash created an external effect directory".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(RecoveredCandidate {
        observation,
        destination_reconciled: true,
        external_effects: recovered.external_effects,
        duplicate_effects: 0,
        exact_fence_acknowledged: false,
        pending_claims: recovered.pending_claims,
        committed_events: Some(recovered.committed_events),
        aggregate_heads: Some(recovered.aggregate_heads),
        journal_sha256: digest::hex(digest::file(&journal)?),
        journal_bytes: journal_metadata.len(),
        effect_sha256: None,
        effect_bytes: None,
        elapsed_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

fn checkpoint_digest(line: &str) -> Result<String, Box<dyn std::error::Error>> {
    let digest = line
        .strip_prefix("peritus-qualification journal-before-stage request_sha256=")
        .ok_or("staged peritusd returned an unknown pre-commit checkpoint")?;
    if !lower_sha256(digest) {
        return Err("staged peritusd returned an invalid pre-commit request digest".into());
    }
    Ok(digest.to_owned())
}

struct ParsedRecovery {
    request_sha256: String,
    journal_verified: bool,
    committed_events: u64,
    aggregate_heads: u64,
    external_effects: u64,
    pending_claims: u64,
}

fn parse_recovery(line: &str) -> Result<ParsedRecovery, Box<dyn std::error::Error>> {
    let fields = line
        .strip_prefix("peritus-qualification journal-before-recover ")
        .ok_or("staged peritusd returned an unknown pre-commit recovery observation")?
        .split_whitespace()
        .collect::<Vec<_>>();
    if fields.len() != 6 {
        return Err("pre-commit recovery observation has the wrong field count".into());
    }
    let request_sha256 = value_field(fields[0], "request_sha256")?.to_owned();
    if !lower_sha256(&request_sha256) {
        return Err("pre-commit recovery returned an invalid request digest".into());
    }
    Ok(ParsedRecovery {
        request_sha256,
        journal_verified: boolean_field(fields[1], "journal_verified")?,
        committed_events: number_field(fields[2], "committed_events")?,
        aggregate_heads: number_field(fields[3], "aggregate_heads")?,
        external_effects: number_field(fields[4], "external_effects")?,
        pending_claims: number_field(fields[5], "pending_claims")?,
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
