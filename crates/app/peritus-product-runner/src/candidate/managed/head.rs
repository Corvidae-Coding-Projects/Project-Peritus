//! Restore nested HEAD without rewriting or deleting any existing branch.

use super::transaction::{Journal, RootFact, projected_fact};
use super::{capture::nested_head, failure, git, recovery, text};
use crate::ProductRunnerError;
use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

mod prepared;
use prepared::Change;

pub(super) struct Prepared {
    change: Change,
    retained: PathBuf,
}

impl Prepared {
    pub(super) fn publish(
        self,
        journal: Option<&mut Journal>,
    ) -> Result<PathBuf, ProductRunnerError> {
        self.change.publish(journal).map_err(|error| {
            failure(format!(
                "nested HEAD restore failed; recovery record at {}: {error}",
                self.retained.display()
            ))
        })?;
        #[cfg(test)]
        super::transaction::fault::pause(super::transaction::fault::Stage::Head);
        Ok(self.retained)
    }
}

pub(super) fn preflight(root: &Path, baseline: Option<&str>) -> Result<(), ProductRunnerError> {
    nested_head(root)?;
    if let Some(commit) = baseline {
        git(root, &["cat-file", "-e", &format!("{commit}^{{commit}}")], None)?;
    }
    Ok(())
}

pub(super) fn prepare(
    root: &Path,
    baseline: Option<&str>,
    restored_root: &Path,
    mut journal: Option<&mut Journal>,
) -> Result<Option<Prepared>, ProductRunnerError> {
    let current = nested_head(root)?;
    let mut target = projected_fact(root, restored_root)?;
    if current.as_deref() == baseline {
        if let Some(journal) = journal {
            journal.head(restored_root.to_path_buf(), target)?;
        }
        return Ok(None);
    }
    let output = super::repository_command(root)?
        .args(["symbolic-ref", "--quiet", "HEAD"])
        .output()
        .map_err(failure)?;
    let branch = if output.status.success() {
        Some(text(output.stdout)?)
    } else if output.status.code() == Some(1) {
        None
    } else {
        return Err(failure(String::from_utf8_lossy(&output.stderr)));
    };
    let git_directory =
        PathBuf::from(text(git(root, &["rev-parse", "--absolute-git-dir"], None)?)?);
    let owner = git_directory.join("peritus");
    recovery::create_directory(&owner)?;
    let directory = owner.join("discarded");
    recovery::create_directory(&directory)?;
    let temporary =
        tempfile::Builder::new().prefix("head-").tempdir_in(&directory).map_err(failure)?;
    let name = temporary
        .path()
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| failure("invalid recovery directory name"))?;
    let saved_ref = format!("refs/peritus/discarded/{name}");
    let unborn_ref = format!("refs/heads/peritus-restored-{name}");
    // Retain detached commits too. Existing branch refs are never moved.
    if let Some(commit) = &current {
        git(root, &["update-ref", &saved_ref, commit, &"0".repeat(commit.len())], None)?;
    }
    let metadata = serde_json::json!({
        "repository": restored_root,
        "previous_head": current,
        "previous_branch": branch,
        "retained_commit_ref": saved_ref,
        "restored_head": baseline,
        "restored_unborn_branch": if baseline.is_none() { Some(&unborn_ref) } else { None },
        "instructions": "Existing branches are unchanged. Discard restores a detached HEAD, or a fresh unborn branch when the baseline had no commit. To revisit the saved commit, use git switch --detach with retained_commit_ref after saving any current work."
    });
    let mut record = fs::File::create(temporary.path().join("head.json")).map_err(failure)?;
    record.write_all(&serde_json::to_vec_pretty(&metadata).map_err(failure)?).map_err(failure)?;
    record.sync_all().map_err(failure)?;
    recovery::sync_directory(temporary.path())?;
    let physical = temporary.keep();
    recovery::sync_directory(&directory)?;
    let retained = physical
        .strip_prefix(root)
        .map_or_else(|_| physical.clone(), |relative| restored_root.join(relative));
    if let RootFact::Repository { commit, symbolic, .. } = &mut target {
        *commit = baseline.map(str::to_owned);
        *symbolic = baseline.is_none().then(|| unborn_ref.clone());
    }
    if let Some(journal) = journal.as_deref_mut() {
        journal.head(restored_root.to_path_buf(), target)?;
        journal.recovery(retained.clone(), None, None)?;
    }
    let change = Change::prepare(root, baseline, current.as_deref(), &unborn_ref, journal)?;
    Ok(Some(Prepared { change, retained }))
}
