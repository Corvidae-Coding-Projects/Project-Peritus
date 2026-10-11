//! Exact source patches across existing and task-created repositories.

use super::{Entry, ManagedBaseline, capture::build_tree, git, text};
use crate::ProductRunnerError;
use std::{collections::BTreeMap, path::Path};

mod instructions;
mod submodule;

use instructions::ExportContext;
use submodule::InitializedSubmodule;

#[derive(Clone, Copy, Debug)]
pub(super) enum SnapshotOrigin {
    Worktree,
    Committed,
}

impl ManagedBaseline {
    pub(super) fn patch_against(
        &self,
        root: &Path,
        current: &Self,
        prefix: &Path,
        owner: &Path,
    ) -> Result<Vec<u8>, ProductRunnerError> {
        let mut context = ExportContext::new(SnapshotOrigin::Worktree);
        let bytes = self.patch_with_context(root, current, prefix, owner, &mut context)?;
        context.finish(bytes)
    }

    pub(crate) fn patch_from_commit(
        &self,
        root: &Path,
        commit: &str,
    ) -> Result<Vec<u8>, ProductRunnerError> {
        let current = submodule::committed_snapshot(root, commit, Some(self))?;
        let mut context = ExportContext::new(SnapshotOrigin::Committed);
        let bytes = self.patch_with_context(
            root,
            &current,
            Path::new(""),
            &super::retention::owner(root)?,
            &mut context,
        )?;
        context.finish(bytes)
    }

    fn patch_with_context(
        &self,
        root: &Path,
        current: &Self,
        prefix: &Path,
        owner: &Path,
        context: &mut ExportContext,
    ) -> Result<Vec<u8>, ProductRunnerError> {
        let display = if prefix.as_os_str().is_empty() {
            String::new()
        } else {
            format!("{}/", super::git_path::tree_name(prefix)?)
        };
        context.record_roots(prefix, self)?;
        let initialized = self.initialized_submodules(root, current, context.origin())?;
        let export_tree = self.export_tree(root, current, &initialized)?;
        let mut original = self.entries.clone();
        for (path, child) in &initialized {
            let prefix = format!("{path}/");
            let observed = original
                .range(prefix.clone()..)
                .take_while(|(path, _)| path.starts_with(&prefix))
                .map(|(path, _)| path.clone())
                .collect::<Vec<_>>();
            for path in observed {
                original.remove(&path);
            }
            original.insert(
                path.clone(),
                Entry { object: child.commit.clone(), mode: "160000".to_owned(), permissions: 0 },
            );
        }
        for (path, child) in &self.nested {
            if initialized_ancestor(path, &initialized) {
                continue;
            }
            if !current.nested.contains_key(path) && child.retained.is_some() {
                original.remove(path);
                copy_retained_entries(root, owner, Path::new(path), child, &mut original)?;
            }
        }
        for (path, entry) in &current.entries {
            if entry.mode == "160000"
                && !original.contains_key(path)
                && let Some(commit) = self.recorded_gitlink(root, path)?
            {
                original.insert(
                    path.clone(),
                    Entry { object: commit, mode: "160000".to_owned(), permissions: 0 },
                );
            }
        }
        let original_tree = build_tree(root, &original)?;
        context.record_paths(root, &display, &original_tree, &export_tree)?;
        let mut bytes = git(
            root,
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--binary",
                &format!("--src-prefix=a/{display}"),
                &format!("--dst-prefix=b/{display}"),
                &original_tree,
                &export_tree,
                "--",
            ],
            None,
        )?;
        for (path, child, now) in self
            .nested
            .iter()
            .filter_map(|(path, child)| current.nested.get(path).map(|now| (path, child, now)))
            .chain(initialized.iter().map(|(path, child)| (path, &child.baseline, &child.current)))
        {
            bytes.extend(child.patch_with_context(
                &root.join(path),
                now,
                &prefix.join(path),
                owner,
                context,
            )?);
        }
        Ok(bytes)
    }

    fn export_tree(
        &self,
        root: &Path,
        current: &Self,
        initialized: &BTreeMap<String, InitializedSubmodule>,
    ) -> Result<String, ProductRunnerError> {
        let mut entries = current.entries.clone();
        for (path, child) in &current.nested {
            if initialized.contains_key(path) {
                // Dependency initialization preserves a recorded submodule boundary;
                // only its source changes are emitted by the separate child diff.
                continue;
            }
            if !self.nested.contains_key(path) {
                entries.remove(path);
                copy_source_entries(root, &root.join(path), Path::new(path), child, &mut entries)?;
            } else if !self.entries.contains_key(path) {
                // A task may make the first commit in an existing unborn repository.
                // Export its source changes below without replacing its directory by a link.
                entries.remove(path);
            }
        }
        build_tree(root, &entries)
    }
}

fn initialized_ancestor(path: &str, initialized: &BTreeMap<String, InitializedSubmodule>) -> bool {
    let mut ancestor = path;
    loop {
        if initialized.contains_key(ancestor) {
            return true;
        }
        let Some((parent, _)) = ancestor.rsplit_once('/') else { return false };
        ancestor = parent;
    }
}

fn copy_retained_entries(
    destination: &Path,
    owner: &Path,
    prefix: &Path,
    baseline: &ManagedBaseline,
    entries: &mut BTreeMap<String, Entry>,
) -> Result<(), ProductRunnerError> {
    let source = super::retention::source_root(owner, baseline)?;
    for (path, entry) in &baseline.entries {
        if entry.mode != "160000" {
            copy_entry(destination, &source, &prefix.join(path), entry, entries)?;
        }
    }
    for (path, child) in &baseline.nested {
        copy_retained_entries(destination, owner, &prefix.join(path), child, entries)?;
    }
    Ok(())
}

fn copy_source_entries(
    destination: &Path,
    source: &Path,
    prefix: &Path,
    baseline: &ManagedBaseline,
    entries: &mut BTreeMap<String, Entry>,
) -> Result<(), ProductRunnerError> {
    for (path, entry) in &baseline.entries {
        if entry.mode == "160000" {
            continue;
        }
        copy_entry(destination, source, &prefix.join(path), entry, entries)?;
    }
    for (path, child) in &baseline.nested {
        copy_source_entries(destination, &source.join(path), &prefix.join(path), child, entries)?;
    }
    Ok(())
}

fn copy_entry(
    destination: &Path,
    source: &Path,
    path: &Path,
    entry: &Entry,
    entries: &mut BTreeMap<String, Entry>,
) -> Result<(), ProductRunnerError> {
    let bytes = git(source, &["cat-file", "blob", &entry.object], None)?;
    let object = text(git(destination, &["hash-object", "-w", "--stdin"], Some(&bytes))?)?;
    let path = super::git_path::tree_name(path)?;
    entries
        .insert(path, Entry { object, mode: entry.mode.clone(), permissions: entry.permissions });
    Ok(())
}
