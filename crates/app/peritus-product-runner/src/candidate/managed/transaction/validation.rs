//! Validate recorded transitions before retrying any source, index, HEAD, or archive effect.

mod directories;
mod paths;

use super::super::{capture, git, retention, text};
use super::{ManagedBaseline, Plan, ProductRunnerError, State, failure, state::Phase};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(in crate::candidate::managed) enum RootFact {
    Absent,
    Directory(u32),
    Other,
    Repository {
        commit: Option<String>,
        symbolic: Option<String>,
        directory: PathBuf,
        permissions: u32,
    },
}

pub(in crate::candidate::managed) fn root_fact(
    root: &Path,
) -> Result<RootFact, ProductRunnerError> {
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(RootFact::Absent),
        Err(error) => return Err(failure(error)),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Ok(RootFact::Other);
    }
    let permissions = crate::file_metadata::permission_fingerprint(&metadata);
    let marker = match fs::symlink_metadata(root.join(".git")) {
        Ok(marker) => marker,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(RootFact::Directory(permissions));
        }
        Err(error) => return Err(failure(error)),
    };
    if marker.file_type().is_symlink() || !(marker.is_dir() || marker.is_file()) {
        return Ok(RootFact::Other);
    }
    let commit = capture::nested_head(root)?;
    let symbolic = symbolic(root)?;
    let directory = PathBuf::from(text(git(root, &["rev-parse", "--absolute-git-dir"], None)?)?)
        .canonicalize()
        .map_err(failure)?;
    Ok(RootFact::Repository { commit, symbolic, directory, permissions })
}

fn symbolic(root: &Path) -> Result<Option<String>, ProductRunnerError> {
    let output = super::super::repository_command(root)?
        .args(["symbolic-ref", "--quiet", "HEAD"])
        .output()
        .map_err(failure)?;
    if output.status.success() {
        return text(output.stdout).map(Some);
    }
    if output.status.code() == Some(1) {
        return Ok(None);
    }
    Err(failure("repository HEAD selection could not be inspected"))
}

pub(super) fn candidate_digest(
    before: &ManagedBaseline,
    head: &str,
) -> Result<[u8; 32], ProductRunnerError> {
    let source = source_snapshot(before);
    let encoded = serde_json::to_vec(&(Some(head), source)).map_err(failure)?;
    let repository = Sha256::digest(encoded).into();
    Ok(crate::progress::WorkspaceCheckpoint::managed_snapshot(head.to_owned(), repository)
        .digest()
        .into_bytes())
}

fn source_snapshot(snapshot: &ManagedBaseline) -> ManagedBaseline {
    let mut source = snapshot.clone();
    source.index.clone_from(&source.tree);
    source.staged = None;
    source.retained = None;
    source.nested =
        source.nested.iter().map(|(name, child)| (name.clone(), source_snapshot(child))).collect();
    source
}

pub(super) fn roots(
    root: &Path,
    before: &ManagedBaseline,
    baseline: &ManagedBaseline,
) -> Result<BTreeMap<PathBuf, RootFact>, ProductRunnerError> {
    let mut names = BTreeSet::from([PathBuf::new()]);
    nested_paths(before, Path::new(""), &mut names);
    nested_paths(baseline, Path::new(""), &mut names);
    names
        .into_iter()
        .map(|name| root_fact(&root.join(&name)).map(|fact| (root.join(name), fact)))
        .collect()
}

fn nested_paths(snapshot: &ManagedBaseline, prefix: &Path, names: &mut BTreeSet<PathBuf>) {
    for (name, child) in &snapshot.nested {
        let path = prefix.join(name);
        names.insert(path.clone());
        nested_paths(child, &path, names);
    }
}

pub(super) fn absolute(path: &Path) -> bool {
    path.is_absolute()
        && path.components().all(|part| {
            matches!(
                part,
                std::path::Component::Prefix(_)
                    | std::path::Component::RootDir
                    | std::path::Component::Normal(_)
            )
        })
}

pub(super) fn structure(plan: &Plan) -> Result<(), ProductRunnerError> {
    let mut expected = BTreeSet::from([PathBuf::new()]);
    nested_paths(&plan.before, Path::new(""), &mut expected);
    nested_paths(&plan.baseline, Path::new(""), &mut expected);
    let expected = expected.into_iter().map(|name| plan.root.join(name)).collect::<BTreeSet<_>>();
    if plan.roots.keys().cloned().collect::<BTreeSet<_>>() != expected
        || !absolute(&plan.root)
        || plan.owners.iter().any(|path| !absolute(path))
    {
        return Err(failure("discard repository ownership is invalid"));
    }
    for fact in plan.roots.values() {
        if let RootFact::Repository { commit, symbolic, directory, permissions } = fact {
            validate_fact(commit.as_deref(), symbolic.as_deref(), directory, *permissions)?;
            if !plan.owners.contains(directory) {
                return Err(failure("discard repository lost its Git owner"));
            }
        }
    }
    Ok(())
}

fn validate_fact(
    commit: Option<&str>,
    symbolic: Option<&str>,
    directory: &Path,
    permissions: u32,
) -> Result<(), ProductRunnerError> {
    if !absolute(directory)
        || permissions > 0o7777
        || commit.is_some_and(|commit| {
            !matches!(commit.len(), 40 | 64) || !commit.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        || symbolic.is_some_and(|name| {
            !name.starts_with("refs/")
                || name.bytes().any(|byte| byte.is_ascii_control())
                || name.contains("..")
        })
    {
        return Err(failure("discard repository fact is invalid"));
    }
    Ok(())
}

pub(super) fn owners(
    root: &Path,
    roots: &BTreeMap<PathBuf, RootFact>,
    baseline: &ManagedBaseline,
) -> Result<BTreeSet<PathBuf>, ProductRunnerError> {
    let mut owners = roots
        .values()
        .filter_map(|fact| match fact {
            RootFact::Repository { directory, .. } => Some(directory.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let owner = retention::owner(root)?;
    owners.extend(retention::linked_owners(&owner, baseline)?);
    let mut names = BTreeSet::new();
    nested_paths(baseline, Path::new(""), &mut names);
    owners.extend(names.into_iter().map(|name| root.join(name).join(".git")));
    Ok(owners)
}

pub(super) fn progress(state: &State) -> Result<(), ProductRunnerError> {
    for (root, fact) in &state.heads {
        head_target(&state.plan, root, fact)?;
    }
    for entry in &state.recovery {
        recovery_path(&state.plan, &entry.path)?;
        if let Some(source) = &entry.source {
            let relative = source.strip_prefix(&state.plan.root).map_err(failure)?;
            if !absolute(source)
                || !state.plan.roots.contains_key(source)
                || !state
                    .plan
                    .paths
                    .iter()
                    .any(|path| relative.starts_with(path) || path.starts_with(relative))
                || !source.starts_with(&state.plan.root)
                || source == &state.plan.root
                || entry.directory_digest.is_none()
            {
                return Err(failure("invalid discarded repository recovery binding"));
            }
        } else if entry.directory_digest.is_some() {
            return Err(failure("invalid HEAD recovery binding"));
        }
    }
    Ok(())
}

pub(super) fn recovery_path(plan: &Plan, path: &Path) -> Result<(), ProductRunnerError> {
    let parent = path.parent().ok_or_else(|| failure("recovery record has no parent"))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| failure("invalid recovery record name"))?;
    if !absolute(path)
        || !name.starts_with("head-") && !name.starts_with("repository-")
        || !plan.owners.iter().any(|owner| {
            parent == owner.join("peritus/discarded")
                || parent.starts_with(owner.join("peritus/restoring"))
        })
    {
        return Err(failure("recovery record is outside its Git owner"));
    }
    Ok(())
}

pub(super) fn head_target(
    plan: &Plan,
    root: &Path,
    fact: &RootFact,
) -> Result<(), ProductRunnerError> {
    let relative = root.strip_prefix(&plan.root).map_err(failure)?;
    let expected = paths::nested_commit(&plan.baseline, relative)
        .ok_or_else(|| failure("HEAD restore names an unowned repository"))?;
    let RootFact::Repository { commit, symbolic, directory, permissions } = fact else {
        return Err(failure("HEAD restore is not a repository"));
    };
    validate_fact(commit.as_deref(), symbolic.as_deref(), directory, *permissions)?;
    if commit.as_deref() != expected.value() || !plan.owners.contains(directory) {
        return Err(failure("HEAD restore does not match its task preimage"));
    }
    Ok(())
}

pub(in crate::candidate::managed) fn projected_fact(
    actual: &Path,
    restored: &Path,
) -> Result<RootFact, ProductRunnerError> {
    let mut fact = root_fact(actual)?;
    if let RootFact::Repository { directory, .. } = &mut fact
        && let Ok(relative) = directory.strip_prefix(actual)
    {
        *directory = restored.join(relative);
    }
    Ok(fact)
}

pub(super) fn transition(state: &State) -> Result<(), ProductRunnerError> {
    inspect(state, false)
}

pub(super) fn postimages(state: &State) -> Result<(), ProductRunnerError> {
    inspect(state, true)
}

fn inspect(state: &State, complete: bool) -> Result<(), ProductRunnerError> {
    let plan = &state.plan;
    let current = ManagedBaseline::capture_with_baseline(&plan.root, true, Some(&plan.before))?;
    let mut gaps = BTreeSet::new();
    for (root, before) in &plan.roots {
        let now = root_fact(root)?;
        let after = after_fact(state, root)?;
        let gap = directories::archived_gap(state, root, &now)?;
        if gap {
            gaps.insert(root.strip_prefix(&plan.root).map_err(failure)?.to_path_buf());
        }
        let archived_before =
            matches!(before, RootFact::Repository { .. } | RootFact::Directory(_))
                && now == RootFact::Absent;
        let restored_plain_directory = matches!(now, RootFact::Directory(_))
            && matches!(before, RootFact::Repository { .. })
            && paths::plain_directory(
                &plan.baseline,
                root.strip_prefix(&plan.root).map_err(failure)?,
            )
            && directories::archive_proven(state, root)?;
        if archived_before && !gap
            || now != after && (complete || &now != before) && !gap && !restored_plain_directory
        {
            return Err(failure(format!(
                "{} changed outside the recorded discard transition; current work was preserved",
                root.display()
            )));
        }
    }
    paths::sources(state, &current, complete, &gaps)?;
    paths::indexes(plan, &current, complete)?;
    if state.phase == Phase::Prepared && complete {
        return Err(failure("discard effects were not admitted"));
    }
    Ok(())
}

fn after_fact(state: &State, root: &Path) -> Result<RootFact, ProductRunnerError> {
    if root == state.plan.root {
        return Ok(state.plan.roots[root].clone());
    }
    if let Some(after) = state.heads.get(root) {
        return Ok(after.clone());
    }
    if paths::snapshot(&state.plan.baseline, root.strip_prefix(&state.plan.root).map_err(failure)?)
        .is_none()
    {
        return Ok(RootFact::Absent);
    }
    // Preparation may not have reached this repository before a crash. Its original
    // state still passes, and the new preparation records the intended destination.
    Ok(state.plan.roots[root].clone())
}

pub(in crate::candidate::managed) use directories::directory_digest;
