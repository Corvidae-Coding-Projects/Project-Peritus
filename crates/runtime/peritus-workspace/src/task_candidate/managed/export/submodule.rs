//! Recover export preimages for a recorded submodule initialized during the task.

use super::super::{Entry, ManagedBaseline, failure, git, text, validate_path};
use super::SnapshotOrigin;
use crate::ProductRunnerError;
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug)]
pub(super) struct InitializedSubmodule {
    pub(super) commit: String,
    pub(super) baseline: ManagedBaseline,
    pub(super) current: ManagedBaseline,
}

impl ManagedBaseline {
    pub(super) fn initialized_submodules(
        &self,
        root: &Path,
        current: &Self,
        origin: SnapshotOrigin,
    ) -> Result<BTreeMap<String, InitializedSubmodule>, ProductRunnerError> {
        let mut initialized = BTreeMap::new();
        for (path, now) in
            current.nested.iter().filter(|(path, _)| !self.nested.contains_key(*path))
        {
            let Some(commit) = self.recorded_gitlink(root, path)? else { continue };
            let mut baseline = committed_preimage(&root.join(path), &commit)?;
            self.observed_submodule_preimages(root, path, &mut baseline)?;
            let current = match origin {
                SnapshotOrigin::Worktree => {
                    Self::capture_with_baseline(&root.join(path), false, Some(&baseline))?
                }
                SnapshotOrigin::Committed => now.clone(),
            };
            initialized.insert(path.clone(), InitializedSubmodule { commit, baseline, current });
        }
        Ok(initialized)
    }

    fn observed_submodule_preimages(
        &self,
        root: &Path,
        path: &str,
        baseline: &mut Self,
    ) -> Result<(), ProductRunnerError> {
        let prefix = format!("{path}/");
        for (name, entry) in
            self.entries.range(prefix.clone()..).take_while(|(name, _)| name.starts_with(&prefix))
        {
            let relative = name
                .strip_prefix(prefix.as_str())
                .ok_or_else(|| failure("invalid child prefix"))?;
            if entry.mode == "160000" {
                baseline.entries.insert(relative.to_owned(), entry.clone());
            } else {
                super::copy_entry(
                    &root.join(path),
                    root,
                    Path::new(relative),
                    entry,
                    &mut baseline.entries,
                )?;
            }
        }
        for (name, child) in
            self.nested.range(prefix.clone()..).take_while(|(name, _)| name.starts_with(&prefix))
        {
            let relative = name
                .strip_prefix(prefix.as_str())
                .ok_or_else(|| failure("invalid child prefix"))?;
            baseline.nested.insert(relative.to_owned(), child.clone());
        }
        baseline.validate()
    }
}

fn committed_preimage(root: &Path, commit: &str) -> Result<ManagedBaseline, ProductRunnerError> {
    committed_tree(root, commit, false, None)
}

pub(super) fn committed_snapshot(
    root: &Path,
    commit: &str,
    prior: Option<&ManagedBaseline>,
) -> Result<ManagedBaseline, ProductRunnerError> {
    committed_tree(root, commit, true, prior)
}

fn committed_tree(
    root: &Path,
    commit: &str,
    include_children: bool,
    prior: Option<&ManagedBaseline>,
) -> Result<ManagedBaseline, ProductRunnerError> {
    if text(git(root, &["cat-file", "-t", commit], None)?)? != "commit" {
        return Err(failure("recorded submodule preimage is not a commit"));
    }
    let tree = text(git(root, &["rev-parse", &format!("{commit}^{{tree}}")], None)?)?;
    let rows = git(root, &["ls-tree", "--full-tree", "-r", "-z", commit], None)?;
    let mut entries = BTreeMap::new();
    let mut nested = BTreeMap::new();
    for row in rows.split(|byte| *byte == 0).filter(|row| !row.is_empty()) {
        let row = std::str::from_utf8(row).map_err(failure)?;
        let (fields, path) =
            row.split_once('\t').ok_or_else(|| failure("missing submodule preimage path"))?;
        validate_path(Path::new(path))?;
        let fields = fields.split(' ').collect::<Vec<_>>();
        let [mode, kind, object] = fields.as_slice() else {
            return Err(failure("invalid submodule preimage entry"));
        };
        if *mode == "160000" && *kind == "commit" {
            // The commit tree retains this child's gitlink for recursive initialization.
            // An uninitialized child has no observed source entry, matching normal capture.
            if include_children {
                let previous =
                    prior.map(|prior| prior.recorded_gitlink(root, path)).transpose()?.flatten();
                let child_root = root.join(path);
                let available = match fs::symlink_metadata(child_root.join(".git")) {
                    Ok(_) => true,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                    Err(error) => return Err(failure(error)),
                };
                if available {
                    let inherited = previous
                        .as_deref()
                        .map(|commit| committed_preimage(&child_root, commit))
                        .transpose()?;
                    let before =
                        prior.and_then(|prior| prior.nested.get(path)).or(inherited.as_ref());
                    nested
                        .insert(path.to_owned(), committed_snapshot(&child_root, object, before)?);
                } else if prior.is_some_and(|prior| prior.nested.contains_key(path))
                    || previous.as_deref().is_some_and(|previous| previous != *object)
                {
                    return Err(failure("recorded committed child repository is unavailable"));
                }
                entries.insert(
                    path.to_owned(),
                    Entry {
                        object: (*object).to_owned(),
                        mode: (*mode).to_owned(),
                        permissions: 0,
                    },
                );
            }
            continue;
        }
        if *kind != "blob" || !matches!(*mode, "100644" | "100755" | "120000") {
            return Err(failure("invalid submodule preimage file mode"));
        }
        entries.insert(
            path.to_owned(),
            Entry { object: (*object).to_owned(), mode: (*mode).to_owned(), permissions: 0 },
        );
    }
    let baseline = ManagedBaseline {
        tree: tree.clone(),
        index: tree,
        staged: None,
        entries,
        nested,
        retained: None,
    };
    baseline.validate()?;
    Ok(baseline)
}
