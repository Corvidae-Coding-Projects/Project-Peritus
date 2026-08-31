//! Exact staged-daemon effects for supported production commit-crash routes.

mod blob;
mod config;
mod journal_before;
mod lease;
mod process;
mod snapshot;

use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use peritus_approval::CredentialRegistrySnapshot;
use peritus_types::RevisionNumber;
use serde::Serialize;

use crate::digest;

use super::args::ControllerPaths;
use super::request::CommitRoute;
use config::{bytes_sha256, create_private_directory, render_configuration, write_new};
use process::{bounded_command, kill_after_checkpoint, one_line};

const EFFECT_DIRECTORY: &str = "outbox-crash-qualification-v1";

pub(super) struct PreparedCandidate {
    pub(super) runtime: RuntimePaths,
    pub(super) journal_head_sha256: String,
    pub(super) version: String,
}

#[derive(Serialize)]
pub(super) struct InjectedCandidate {
    pub(super) checkpoint: String,
    pub(super) claim_fence: Option<u64>,
    pub(super) request_sha256: Option<String>,
    pub(super) effect_path: Option<String>,
    pub(super) effect_sha256: Option<String>,
    pub(super) effect_bytes: Option<u64>,
    pub(super) artifact_sha256: Option<String>,
    pub(super) artifact_bytes: Option<u64>,
    pub(super) snapshot: Option<SnapshotObservation>,
    pub(super) lease: Option<LeaseObservation>,
    pub(super) killed_exit: String,
}

#[derive(Serialize)]
pub(super) struct RecoveredCandidate {
    pub(super) observation: String,
    pub(super) destination_reconciled: bool,
    pub(super) external_effects: u64,
    pub(super) duplicate_effects: u64,
    pub(super) exact_fence_acknowledged: bool,
    pub(super) pending_claims: u64,
    pub(super) committed_events: Option<u64>,
    pub(super) aggregate_heads: Option<u64>,
    pub(super) journal_sha256: String,
    pub(super) journal_bytes: u64,
    pub(super) effect_sha256: Option<String>,
    pub(super) effect_bytes: Option<u64>,
    pub(super) artifact_sha256: Option<String>,
    pub(super) artifact_bytes: Option<u64>,
    pub(super) snapshot: Option<SnapshotObservation>,
    pub(super) lease: Option<LeaseObservation>,
    pub(super) elapsed_millis: u64,
}

#[derive(Serialize)]
pub(super) struct SnapshotObservation {
    pub(super) commit: Option<String>,
    pub(super) tree: String,
    pub(super) reference: String,
    pub(super) manifest_sha256: Option<String>,
}

#[derive(Serialize)]
pub(super) struct LeaseObservation {
    pub(super) request_sha256: String,
    pub(super) state_revision: Option<u64>,
    pub(super) state_sha256: Option<String>,
    pub(super) producing_position: Option<u64>,
}

pub(super) struct RuntimePaths {
    root: PathBuf,
    state: PathBuf,
    config: PathBuf,
}

pub(super) fn prepare(
    paths: &ControllerPaths,
) -> Result<PreparedCandidate, Box<dyn std::error::Error>> {
    let root = paths.subject_root.join("h1-controller-runtime");
    create_private_directory(&root)?;
    let state = root.join("state");
    create_private_directory(&state)?;
    let registry = root.join("approval-registry.bin");
    let snapshot = CredentialRegistrySnapshot::new(RevisionNumber::first(), Vec::new())
        .map_err(|error| format!("construct H1 approval registry: {error:?}"))?;
    let registry_bytes = snapshot
        .canonical_bytes()
        .map_err(|error| format!("encode H1 approval registry: {error:?}"))?;
    write_new(&registry, &registry_bytes)?;
    let config = root.join("peritus.toml");
    let configuration = render_configuration(&state, &registry, &paths.build_sha256);
    write_new(&config, configuration.as_bytes())?;
    let output = bounded_command(
        &paths.candidate,
        [OsStr::new("--version")],
        &root,
        &root.join("version.stdout"),
        &root.join("version.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd version probe failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let version = one_line(&output.stdout, "peritusd version")?;
    if !version.starts_with("peritusd ") {
        return Err("staged candidate returned an unknown version identity".into());
    }
    let mut baseline = Vec::with_capacity(configuration.len() + version.len() + 64);
    baseline.extend_from_slice(b"peritus/h1/journal-baseline/v1\0");
    baseline.extend_from_slice(configuration.as_bytes());
    baseline.extend_from_slice(version.as_bytes());
    let journal_head_sha256 = bytes_sha256(&baseline);
    Ok(PreparedCandidate {
        runtime: RuntimePaths { root, state, config },
        journal_head_sha256,
        version,
    })
}

pub(super) fn inject(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
    route: CommitRoute,
) -> Result<InjectedCandidate, Box<dyn std::error::Error>> {
    match route {
        CommitRoute::BlobBeforeDurableCommit | CommitRoute::BlobAfterDurableCommitBeforeAck => {
            return blob::inject(paths, runtime, route);
        }
        CommitRoute::JournalBeforeDurableCommit => {
            return journal_before::inject(paths, runtime);
        }
        CommitRoute::LeaseBeforeDurableCommit | CommitRoute::LeaseAfterDurableCommitBeforeAck => {
            return lease::inject(paths, runtime, route);
        }
        CommitRoute::SnapshotBeforeDurableCommit
        | CommitRoute::SnapshotAfterDurableCommitBeforeAck => {
            return snapshot::inject(paths, runtime, route);
        }
        CommitRoute::JournalAfterDurableCommitBeforeAck => {}
    }
    let stderr_path = runtime.root.join("inject.stderr");
    let killed = kill_after_checkpoint(
        &paths.candidate,
        [OsStr::new("qualify-outbox-stage"), OsStr::new("--config"), runtime.config.as_os_str()],
        &runtime.root,
        &stderr_path,
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
        killed_exit: killed.status,
    })
}

pub(super) fn recover(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
    injected: &InjectedCandidate,
    route: CommitRoute,
) -> Result<RecoveredCandidate, Box<dyn std::error::Error>> {
    match route {
        CommitRoute::BlobBeforeDurableCommit | CommitRoute::BlobAfterDurableCommitBeforeAck => {
            return blob::recover(paths, runtime, injected, route);
        }
        CommitRoute::JournalBeforeDurableCommit => {
            return journal_before::recover(paths, runtime, injected);
        }
        CommitRoute::LeaseBeforeDurableCommit | CommitRoute::LeaseAfterDurableCommitBeforeAck => {
            return lease::recover(paths, runtime, injected, route);
        }
        CommitRoute::SnapshotBeforeDurableCommit
        | CommitRoute::SnapshotAfterDurableCommitBeforeAck => {
            return snapshot::recover(paths, runtime, injected, route);
        }
        CommitRoute::JournalAfterDurableCommitBeforeAck => {}
    }
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
    if !recovered.destination_reconciled
        || recovered.external_effects != 1
        || recovered.duplicate_effects != 0
        || !recovered.exact_fence_acknowledged
        || recovered.pending_claims != 0
    {
        return Err("staged peritusd did not reconcile and settle the exact outbox effect".into());
    }
    let injected_effect = injected
        .effect_path
        .as_deref()
        .ok_or("journal after-commit checkpoint omitted its effect path")?;
    let effect = fs::canonicalize(injected_effect)?;
    let effect_sha256 = digest::hex(digest::file(&effect)?);
    let effect_bytes = fs::metadata(&effect)?.len();
    if Some(&effect_sha256) != injected.effect_sha256.as_ref()
        || Some(effect_bytes) != injected.effect_bytes
    {
        return Err("recovery changed or replaced the identity-bearing outbox effect".into());
    }
    let effect_entries =
        fs::read_dir(runtime.state.join(EFFECT_DIRECTORY))?.collect::<Result<Vec<_>, _>>()?;
    if effect_entries.len() != 1
        || !effect_entries[0].file_type()?.is_file()
        || fs::canonicalize(effect_entries[0].path())? != effect
    {
        return Err("recovery left a duplicate or non-file outbox effect".into());
    }
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
        elapsed_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

pub(super) fn cleanup(runtime: &RuntimePaths) -> Result<(), Box<dyn std::error::Error>> {
    let root = fs::canonicalize(&runtime.root)?;
    if root.parent().is_none() || root.file_name() != Some(OsStr::new("h1-controller-runtime")) {
        return Err("H1 controller refused an unexpected cleanup root".into());
    }
    fs::remove_dir_all(&root)?;
    if root.exists() {
        return Err("H1 controller runtime remained after cleanup".into());
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
