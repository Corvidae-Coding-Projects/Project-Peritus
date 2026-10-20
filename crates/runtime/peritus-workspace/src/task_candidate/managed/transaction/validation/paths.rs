//! Source and index fences preserve edits outside the explicitly owned path set.

use super::super::super::Entry;
use super::{ManagedBaseline, Plan, ProductRunnerError, State, failure};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub(super) fn snapshot<'a>(value: &'a ManagedBaseline, path: &Path) -> Option<&'a ManagedBaseline> {
    if path.as_os_str().is_empty() {
        return Some(value);
    }
    value.nested.iter().find_map(|(prefix, child)| {
        path.strip_prefix(prefix).ok().and_then(|remaining| snapshot(child, remaining))
    })
}

pub(super) enum NestedCommit<'a> {
    Unborn,
    Committed(&'a str),
}

impl<'a> NestedCommit<'a> {
    pub(super) const fn value(&self) -> Option<&'a str> {
        match self {
            Self::Unborn => None,
            Self::Committed(commit) => Some(commit),
        }
    }
}

pub(super) fn nested_commit<'a>(
    value: &'a ManagedBaseline,
    path: &Path,
) -> Option<NestedCommit<'a>> {
    for (prefix, child) in &value.nested {
        if path == Path::new(prefix) {
            return Some(value.entries.get(prefix).map_or(NestedCommit::Unborn, |entry| {
                NestedCommit::Committed(entry.object.as_str())
            }));
        }
        if let Ok(remaining) = path.strip_prefix(prefix)
            && let Some(commit) = nested_commit(child, remaining)
        {
            return Some(commit);
        }
    }
    None
}

fn flatten(value: &ManagedBaseline, prefix: &Path, entries: &mut BTreeMap<PathBuf, Entry>) {
    for (name, entry) in &value.entries {
        if entry.mode != "160000" {
            entries.insert(prefix.join(name), entry.clone());
        }
    }
    for (name, child) in &value.nested {
        flatten(child, &prefix.join(name), entries);
    }
}

fn owned(plan: &Plan, path: &Path) -> bool {
    plan.paths.iter().any(|scope| path.starts_with(scope))
}

pub(super) fn sources(
    state: &State,
    current: &ManagedBaseline,
    complete: bool,
    gaps: &BTreeSet<PathBuf>,
) -> Result<(), ProductRunnerError> {
    let plan = &state.plan;
    let mut before = BTreeMap::new();
    let mut after = BTreeMap::new();
    let mut now = BTreeMap::new();
    flatten(&plan.before, Path::new(""), &mut before);
    flatten(&plan.baseline, Path::new(""), &mut after);
    flatten(current, Path::new(""), &mut now);
    let names = before.keys().chain(after.keys()).chain(now.keys()).collect::<BTreeSet<_>>();
    for path in names.into_iter().filter(|path| owned(plan, path)) {
        let owned_move_gap = !complete
            && !now.contains_key(path)
            && gaps.iter().any(|prefix| path.starts_with(prefix));
        if now.get(path) != after.get(path)
            && (complete || now.get(path) != before.get(path))
            && !owned_move_gap
        {
            return Err(failure(format!(
                "{} changed outside the recorded discard transition; current bytes were preserved",
                path.display()
            )));
        }
    }
    // New repositories move as one tree, so partial edits inside a still-present
    // new repository cannot be mistaken for partially published file restorations.
    whole_archives(plan, current, &before, &now)
}

fn whole_archives(
    plan: &Plan,
    current: &ManagedBaseline,
    before: &BTreeMap<PathBuf, Entry>,
    now: &BTreeMap<PathBuf, Entry>,
) -> Result<(), ProductRunnerError> {
    for root in plan.roots.keys() {
        let prefix = root.strip_prefix(&plan.root).map_err(failure)?;
        if prefix.as_os_str().is_empty() || !owned(plan, prefix) {
            continue;
        }
        let original = snapshot(&plan.before, prefix);
        let target = snapshot(&plan.baseline, prefix);
        let present = snapshot(current, prefix);
        let before_move = original.is_some() && target.is_none() && present.is_some()
            || original.is_none() && target.is_some() && present.is_none() && root.is_dir();
        if before_move {
            let original = before
                .iter()
                .filter(|(name, _)| name.starts_with(prefix))
                .collect::<BTreeMap<_, _>>();
            let present =
                now.iter().filter(|(name, _)| name.starts_with(prefix)).collect::<BTreeMap<_, _>>();
            if original != present {
                return Err(failure(
                    "new repository changed before its archive; current work was preserved",
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn plain_directory(baseline: &ManagedBaseline, prefix: &Path) -> bool {
    let mut entries = BTreeMap::new();
    flatten(baseline, Path::new(""), &mut entries);
    snapshot(baseline, prefix).is_none() && entries.keys().any(|path| path.starts_with(prefix))
}

pub(super) fn indexes(
    plan: &Plan,
    current: &ManagedBaseline,
    complete: bool,
) -> Result<(), ProductRunnerError> {
    for root in plan.roots.keys() {
        let prefix = root.strip_prefix(&plan.root).map_err(failure)?;
        let Some(now) = snapshot(current, prefix) else { continue };
        let before = snapshot(&plan.before, prefix);
        let after = snapshot(&plan.baseline, prefix);
        if after.is_none() {
            if before.is_none_or(|before| before.staged != now.staged) {
                return Err(failure("new repository index changed before its archive"));
            }
            continue;
        }
        let mut names = BTreeSet::new();
        for value in [before, after, Some(now)].into_iter().flatten() {
            names.extend(value.staged.iter().flatten().map(|entry| entry.path().to_owned()));
        }
        names.extend(
            plan.paths
                .iter()
                .filter_map(|name| name.strip_prefix(prefix).ok())
                .filter_map(|name| name.to_str())
                .map(str::to_owned),
        );
        for name in names.into_iter().filter(|name| owned(plan, &prefix.join(name))) {
            let present = now.staged_at(root, &name)?;
            let target =
                after.map(|after| after.staged_at(root, &name)).transpose()?.unwrap_or_default();
            let original =
                before.map(|before| before.staged_at(root, &name)).transpose()?.unwrap_or_default();
            if present != target && (complete || present != original) {
                return Err(failure(format!(
                    "Git index entry {} changed outside the discard transition",
                    prefix.join(name).display()
                )));
            }
        }
    }
    Ok(())
}
