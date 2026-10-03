//! Export complete source trees for repositories created inside a task workspace.

use super::{Entry, ManagedBaseline, capture::build_tree, git, text};
use crate::ProductRunnerError;
use std::{collections::BTreeMap, path::Path};

impl ManagedBaseline {
    pub(super) fn patch_against(
        &self,
        root: &Path,
        current: &Self,
        prefix: &Path,
        owner: &Path,
    ) -> Result<Vec<u8>, ProductRunnerError> {
        let display = if prefix.as_os_str().is_empty() {
            String::new()
        } else {
            format!("{}/", super::git_path::tree_name(prefix)?)
        };
        let export_tree = self.export_tree(root, current)?;
        let mut original = self.entries.clone();
        for (path, child) in &self.nested {
            if !current.nested.contains_key(path) && child.retained.is_some() {
                original.remove(path);
                copy_retained_entries(root, owner, Path::new(path), child, &mut original)?;
            }
        }
        let original_tree = build_tree(root, &original)?;
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
        for (path, child) in &self.nested {
            if let Some(now) = current.nested.get(path) {
                bytes.extend(child.patch_against(
                    &root.join(path),
                    now,
                    &prefix.join(path),
                    owner,
                )?);
            }
        }
        Ok(bytes)
    }

    fn export_tree(&self, root: &Path, current: &Self) -> Result<String, ProductRunnerError> {
        let mut entries = current.entries.clone();
        for (path, child) in &current.nested {
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
