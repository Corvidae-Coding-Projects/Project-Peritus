//! Staged-daemon effects for retained Git snapshot commit recovery.

use std::ffi::OsStr;
use std::fs;
use std::time::Instant;

use crate::digest;

use super::process::{bounded_command, kill_after_checkpoint, one_line};
use super::{InjectedCandidate, RecoveredCandidate, RuntimePaths, SnapshotObservation};
use crate::native_controller::args::{ControllerPaths, lower_sha256};
use crate::native_controller::request::CommitRoute;

pub(super) fn inject(
    paths: &ControllerPaths,
    runtime: &RuntimePaths,
    route: CommitRoute,
) -> Result<InjectedCandidate, Box<dyn std::error::Error>> {
    let (command, prefix, count) = match route {
        CommitRoute::SnapshotBeforeDurableCommit => {
            ("qualify-snapshot-before-stage", "peritus-qualification snapshot-before-stage ", 2)
        }
        CommitRoute::SnapshotAfterDurableCommitBeforeAck => {
            ("qualify-snapshot-after-stage", "peritus-qualification snapshot-after-stage ", 5)
        }
        _ => return Err("snapshot controller received a non-snapshot route".into()),
    };
    let killed = kill_after_checkpoint(
        &paths.candidate,
        [OsStr::new(command), OsStr::new("--config"), runtime.config.as_os_str()],
        &runtime.root,
        &runtime.root.join("inject.stderr"),
    )?;
    let fields = fields(&killed.line, prefix, count)?;
    let (commit, tree_index, reference_index, manifest_sha256) = match route {
        CommitRoute::SnapshotBeforeDurableCommit => (None, 0, 1, None),
        CommitRoute::SnapshotAfterDurableCommitBeforeAck => {
            let commit = value_field(fields[0], "commit")?.to_owned();
            require_object_id(&commit)?;
            if !boolean_field(fields[4], "retained")? {
                return Err("post-commit snapshot checkpoint is not retained".into());
            }
            let manifest = value_field(fields[3], "manifest_sha256")?.to_owned();
            if !lower_sha256(&manifest) {
                return Err("snapshot manifest digest is not canonical SHA-256".into());
            }
            (Some(commit), 1, 2, Some(manifest))
        }
        _ => return Err("snapshot controller received a non-snapshot route".into()),
    };
    let tree = value_field(fields[tree_index], "tree")?.to_owned();
    require_object_id(&tree)?;
    let reference = value_field(fields[reference_index], "reference")?.to_owned();
    require_reference(&reference)?;
    independently_verify(runtime, commit.as_deref(), &reference, manifest_sha256.as_deref())?;
    Ok(InjectedCandidate {
        checkpoint: killed.line,
        claim_fence: None,
        request_sha256: None,
        effect_path: None,
        effect_sha256: None,
        effect_bytes: None,
        artifact_sha256: None,
        artifact_bytes: None,
        snapshot: Some(SnapshotObservation { commit, tree, reference, manifest_sha256 }),
        lease: None,
        patch: None,
        gate: None,
        promotion: None,
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
    let (command, prefix, committed) = match route {
        CommitRoute::SnapshotBeforeDurableCommit => (
            "qualify-snapshot-before-recover",
            "peritus-qualification snapshot-before-recover ",
            false,
        ),
        CommitRoute::SnapshotAfterDurableCommitBeforeAck => (
            "qualify-snapshot-after-recover",
            "peritus-qualification snapshot-after-recover ",
            true,
        ),
        _ => return Err("snapshot controller received a non-snapshot route".into()),
    };
    let output = bounded_command(
        &paths.candidate,
        [OsStr::new(command), OsStr::new("--config"), runtime.config.as_os_str()],
        &runtime.root,
        &runtime.root.join("recover.stdout"),
        &runtime.root.join("recover.stderr"),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "staged peritusd snapshot recovery failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let observation = one_line(&output.stdout, "snapshot recovery")?;
    let values = fields(&observation, prefix, 7)?;
    let commit = optional_field(values[0], "commit")?;
    let tree = value_field(values[1], "tree")?.to_owned();
    let reference = value_field(values[2], "reference")?.to_owned();
    let manifest_sha256 = optional_field(values[3], "manifest_sha256")?;
    let journal_verified = boolean_field(values[4], "journal_verified")?;
    let retained = boolean_field(values[5], "retained")?;
    let snapshot_refs = number_field(values[6], "snapshot_refs")?;
    let injected_snapshot =
        injected.snapshot.as_ref().ok_or("snapshot checkpoint identity missing")?;
    if commit != injected_snapshot.commit
        || tree != injected_snapshot.tree
        || reference != injected_snapshot.reference
        || manifest_sha256 != injected_snapshot.manifest_sha256
        || !journal_verified
        || retained != committed
        || snapshot_refs != u64::from(committed)
    {
        return Err("staged peritusd snapshot recovery differs from the commit boundary".into());
    }
    independently_verify(runtime, commit.as_deref(), &reference, manifest_sha256.as_deref())?;
    let journal = runtime.state.join("peritus.sqlite3");
    let journal_metadata = fs::symlink_metadata(&journal)?;
    if !journal_metadata.file_type().is_file() || journal_metadata.len() == 0 {
        return Err("snapshot recovery journal is missing or empty".into());
    }
    Ok(RecoveredCandidate {
        observation,
        destination_reconciled: true,
        external_effects: snapshot_refs,
        duplicate_effects: 0,
        exact_fence_acknowledged: false,
        pending_claims: 0,
        committed_events: Some(0),
        aggregate_heads: Some(0),
        journal_sha256: digest::hex(digest::file(&journal)?),
        journal_bytes: journal_metadata.len(),
        effect_sha256: None,
        effect_bytes: None,
        artifact_sha256: None,
        artifact_bytes: None,
        snapshot: Some(SnapshotObservation { commit, tree, reference, manifest_sha256 }),
        lease: None,
        patch: None,
        gate: None,
        promotion: None,
        elapsed_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

fn independently_verify(
    runtime: &RuntimePaths,
    commit: Option<&str>,
    reference: &str,
    manifest_sha256: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    require_reference(reference)?;
    reference.strip_prefix("refs/").ok_or("snapshot reference prefix missing")?;
    let git_dir =
        runtime.state.join("snapshot-crash-qualification-v1").join("repository").join(".git");
    let loose_ref = git_dir.join(reference);
    match (commit, fs::read(&loose_ref)) {
        (None, Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
        (Some(expected), Ok(bytes)) => {
            let observed = std::str::from_utf8(&bytes)?.trim_end_matches(['\r', '\n']);
            if observed != expected {
                return Err("controller observed a different retained snapshot commit".into());
            }
        }
        _ => return Err("controller observed unexpected retained snapshot state".into()),
    }
    let manifest =
        runtime.state.join("snapshot-crash-qualification-v1").join("snapshot-manifest-v1.bin");
    match (manifest_sha256, fs::read(manifest)) {
        (None, Err(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        (Some(expected), Ok(bytes)) if digest::hex(digest::bytes(&bytes)) == expected => Ok(()),
        _ => Err("controller observed unexpected snapshot manifest state".into()),
    }
}

fn fields<'a>(
    line: &'a str,
    prefix: &str,
    count: usize,
) -> Result<Vec<&'a str>, Box<dyn std::error::Error>> {
    let fields = line
        .strip_prefix(prefix)
        .ok_or("staged peritusd returned an unknown snapshot observation")?
        .split_whitespace()
        .collect::<Vec<_>>();
    if fields.len() != count {
        return Err("snapshot observation has the wrong field count".into());
    }
    Ok(fields)
}

fn optional_field(field: &str, name: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let value = value_field(field, name)?;
    if value == "none" { Ok(None) } else { Ok(Some(value.to_owned())) }
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

fn require_object_id(value: &str) -> Result<(), Box<dyn std::error::Error>> {
    if matches!(value.len(), 40 | 64)
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err("snapshot object identity is not canonical lowercase hexadecimal".into())
    }
}

fn require_reference(value: &str) -> Result<(), Box<dyn std::error::Error>> {
    let prefix = "refs/peritus/workspaces/";
    if value.starts_with(prefix)
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'/')
    {
        Ok(())
    } else {
        Err("snapshot reference is not a canonical Peritus ref".into())
    }
}
