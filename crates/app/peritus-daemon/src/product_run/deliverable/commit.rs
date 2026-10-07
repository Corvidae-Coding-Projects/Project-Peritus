//! Commit each selected file in the repository that owns it.

use super::{ProductDeliverable, ProductRunServiceError};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

mod attempt;
mod handoff;

pub(in crate::product_run) fn attempt_matches(
    directory: &Path,
    record: &super::super::RunRecord,
    deliverable: &ProductDeliverable,
) -> Result<bool, ProductRunServiceError> {
    attempt::matches(directory, record, deliverable)
}

pub(super) fn validate_retry(
    directory: &Path,
    record: &mut super::super::RunRecord,
    deliverable: &ProductDeliverable,
    workspace: &Path,
) -> Result<ProductDeliverable, ProductRunServiceError> {
    match super::validate_exact_candidate(record, deliverable, workspace) {
        Ok(()) => Ok(deliverable.clone()),
        Err(error) => {
            if deliverable.discarded()
                || !deliverable.commit_revision().is_empty()
                || Path::new(deliverable.workspace_path()) != workspace
                || !attempt::matches(directory, record, deliverable)?
            {
                return Err(error);
            }
            let current = peritus_product_runner::ProductRunner::candidate_digest(workspace)
                .map_err(|error| failure(workspace, "inspect commit retry", error))?;
            let content = peritus_product_runner::ProductRunner::candidate_source_digest(workspace)
                .map_err(|error| failure(workspace, "inspect commit retry source", error))?;
            // History changed during the prior attempt. Reconcile the new repository context
            // while retaining evidence whose declared source and execution inputs still match.
            let execution =
                record.checkpoint.and_then(|checkpoint| checkpoint.identity().execution_digest());
            super::super::recovery::mark_stale(record, content, current, execution)?;
            record.snapshot.deliverable().cloned().ok_or(ProductRunServiceError::InvalidState)
        }
    }
}

pub(super) fn prepare_recovery(
    directory: &Path,
    record: &super::super::RunRecord,
    deliverable: ProductDeliverable,
) -> Result<(ProductDeliverable, String), ProductRunServiceError> {
    if record.task_baseline_required && record.task_baseline.is_none() {
        return Err(ProductRunServiceError::WorkspaceUnavailable);
    }
    // Save the exact source patch before commits change nested HEADs. The patch
    // remains usable if a hook, signer, or later repository commit fails.
    let patch = if attempt::matches(directory, record, &deliverable)? {
        PathBuf::from(deliverable.export_path())
    } else {
        super::export_deliverable(
            directory,
            record.request.run_id(),
            &deliverable,
            record.task_baseline.as_deref(),
        )?
    };
    let display = patch.to_string_lossy().into_owned();
    let deliverable = deliverable
        .mark_exported(display.clone())
        .map_err(|_| ProductRunServiceError::InvalidMessage)?;
    attempt::save(directory, record, &deliverable)?;
    Ok((
        deliverable,
        format!("Patch saved to {display}; committing selected files"),
    ))
}

pub(super) fn commit_prepared(
    directory: &Path,
    record: &mut super::super::RunRecord,
    deliverable: ProductDeliverable,
) -> Result<(ProductDeliverable, String), ProductRunServiceError> {
    let display = deliverable.export_path().to_owned();
    record.snapshot = super::replace_snapshot(
        &record.snapshot,
        record.snapshot.phase(),
        &format!("Patch saved to {display}; committing selected files"),
        record.snapshot.summary(),
    )?
    .with_deliverable(deliverable.clone());
    let description = if record.snapshot.summary().trim().is_empty() {
        record.request.display_task()
    } else {
        record.snapshot.summary()
    };
    let revision = commit_deliverable(&deliverable, description)?;
    let deliverable = deliverable
        .mark_committed(revision.clone())
        .map_err(|_| ProductRunServiceError::InvalidMessage)?;
    let (deliverable, detail) = handoff::bind(directory, record, deliverable)?;
    Ok((
        deliverable,
        format!("Deliverable committed as {revision}; source patch saved to {display}{detail}"),
    ))
}

#[cfg(test)]
pub(super) fn with_recovery(
    directory: &Path,
    record: &mut super::super::RunRecord,
    deliverable: ProductDeliverable,
) -> Result<(ProductDeliverable, String), ProductRunServiceError> {
    let (deliverable, status) = prepare_recovery(directory, record, deliverable)?;
    record.snapshot = super::replace_snapshot(
        &record.snapshot,
        record.snapshot.phase(),
        &status,
        record.snapshot.summary(),
    )?
    .with_deliverable(deliverable.clone());
    super::persist_record(directory, record)?;
    match commit_prepared(directory, record, deliverable) {
        Ok(committed) => Ok(committed),
        Err(error) => {
            let detail = error.to_string();
            let detail = &detail[..detail.floor_char_boundary(detail.len().min(8192))];
            let display = record
                .snapshot
                .deliverable()
                .map(ProductDeliverable::export_path)
                .unwrap_or_default();
            record.snapshot = super::replace_snapshot(
                &record.snapshot,
                record.snapshot.phase(),
                &format!("Commit did not complete: {detail}. Source patch saved to {display}"),
                record.snapshot.summary(),
            )?;
            super::persist_record(directory, record)?;
            Err(error)
        }
    }
}

pub(super) fn commit_deliverable(
    deliverable: &ProductDeliverable,
    task: &str,
) -> Result<String, ProductRunServiceError> {
    let subject = task.lines().next().unwrap_or("completed task").trim();
    let subject = &subject[..subject.floor_char_boundary(subject.len().min(64))];
    let message = format!("peritus: {subject}");
    let paths = deliverable.changed_paths().iter().map(PathBuf::from).collect::<Vec<_>>();
    commit_repository(Path::new(deliverable.workspace_path()), &paths, &message)
}

fn commit_repository(
    root: &Path,
    paths: &[PathBuf],
    message: &str,
) -> Result<String, ProductRunServiceError> {
    let mut local = BTreeSet::new();
    let mut nested: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    for path in paths {
        if let Some(prefix) = nested_root(root, path)? {
            local.insert(prefix.clone());
            let children = nested.entry(prefix.clone()).or_default();
            let child =
                path.strip_prefix(prefix).map_err(|_| ProductRunServiceError::InvalidState)?;
            if !child.as_os_str().is_empty() {
                children.push(child.to_path_buf());
            }
        } else {
            local.insert(path.clone());
        }
    }
    for (path, children) in nested {
        if !children.is_empty() {
            commit_repository(&root.join(path), &children, message)?;
        }
    }
    stage_selected(root, &local)?;
    let difference = command(root)
        .args(["diff", "--cached", "--quiet", "--ignore-submodules=none", "--"])
        .args(&local)
        .output()
        .map_err(|error| failure(root, "inspect staged deliverable", error))?;
    match difference.status.code() {
        Some(0) => {}
        Some(1) => {
            let mut commit = command(root);
            commit.args(["commit", "--only", "-m", message, "--"]).args(&local);
            checked(&mut commit, "commit deliverable")?;
        }
        _ => {
            return Err(failure(
                root,
                "inspect staged deliverable",
                String::from_utf8_lossy(&difference.stderr),
            ));
        }
    }
    let revision = checked(
        command(root).args(["rev-parse", "--verify", "HEAD"]),
        "resolve committed deliverable",
    )?;
    String::from_utf8(revision.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(|error| failure(root, "decode committed deliverable", error))
}

fn stage_selected(root: &Path, paths: &BTreeSet<PathBuf>) -> Result<(), ProductRunServiceError> {
    let mut stage = Vec::new();
    for path in paths {
        let exists = match fs::symlink_metadata(root.join(path)) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(failure(root, "inspect deliverable path", error)),
        };
        if exists {
            stage.push(path);
            continue;
        }
        let tracked = checked(
            command(root).args(["ls-files", "-z", "--cached", "--"]).arg(path),
            "inspect deliverable staging",
        )?;
        if !tracked.stdout.is_empty() {
            stage.push(path);
        }
    }
    // A task may already have staged or committed a deletion. Passing that
    // missing path to git add would reject an otherwise valid handoff.
    if !stage.is_empty() {
        checked(command(root).args(["add", "--"]).args(stage), "stage deliverable")?;
    }
    Ok(())
}

fn nested_root(root: &Path, path: &Path) -> Result<Option<PathBuf>, ProductRunServiceError> {
    let mut prefix = PathBuf::new();
    for component in path.components() {
        prefix.push(component);
        let absolute = root.join(&prefix);
        match fs::symlink_metadata(&absolute) {
            Ok(metadata) if !metadata.is_dir() => break,
            Ok(_) if absolute.join(".git").exists() => return Ok(Some(prefix)),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(failure(root, "locate deliverable repository", error)),
        }
    }
    Ok(None)
}

fn command(root: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(root).env("GIT_LITERAL_PATHSPECS", "1");
    command
}

fn checked(
    command: &mut Command,
    operation: &'static str,
) -> Result<Output, ProductRunServiceError> {
    let root = command.get_current_dir().unwrap_or_else(|| Path::new("."));
    let root = root.to_path_buf();
    let output = command.output().map_err(|error| failure(&root, operation, error))?;
    if !output.status.success() {
        return Err(failure(&root, operation, String::from_utf8_lossy(&output.stderr)));
    }
    Ok(output)
}

fn failure(
    root: &Path,
    operation: &'static str,
    error: impl std::fmt::Display,
) -> ProductRunServiceError {
    ProductRunServiceError::internal(
        operation,
        format!("Git operation in {} failed: {error}", root.display()),
    )
}
