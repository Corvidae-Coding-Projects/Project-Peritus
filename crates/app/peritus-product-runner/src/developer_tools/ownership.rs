//! Workspace file ownership retained across one complete product run.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use peritus_agent::DeveloperLoopError;

use super::path::{ignored, tool};

/// Distinguishes baseline and model-caused files from unrelated late external evidence.
#[derive(Clone)]
pub struct WorkspaceOwnership {
    baseline: BTreeSet<PathBuf>,
    directly_created: BTreeSet<PathBuf>,
    command_created: BTreeSet<PathBuf>,
}

impl WorkspaceOwnership {
    /// Captures regular files already present when the product run begins.
    #[must_use]
    pub fn capture(root: &Path) -> Self {
        let baseline = regular_files(root);
        Self { baseline, directly_created: BTreeSet::new(), command_created: BTreeSet::new() }
    }

    /// Captures files that existed outside the run's current ownership immediately before a
    /// structured command. A later comparison attributes only files newly produced by that
    /// command, preserving unrelated files that appeared through another actor.
    #[must_use]
    pub(super) fn unowned_files(&self, root: &Path) -> BTreeSet<PathBuf> {
        untracked_files(root)
            .unwrap_or_else(|| regular_files(root))
            .into_iter()
            .filter(|path| {
                !self.baseline.contains(path)
                    && !self.directly_created.contains(path)
                    && !self.command_created.contains(path)
            })
            .collect()
    }

    /// Records regular files that appeared while one harness-owned command was executing.
    pub(super) fn record_command_creations(
        &mut self,
        root: &Path,
        unowned_before: &BTreeSet<PathBuf>,
    ) {
        for path in self.unowned_files(root).difference(unowned_before) {
            self.command_created.insert(path.clone());
        }
    }

    /// Records a new file created through the explicit text-write tool.
    pub fn record_direct_creation(&mut self, path: &Path, existed_before: bool) {
        if !existed_before {
            self.directly_created.insert(path.to_path_buf());
        }
    }

    /// Returns whether source layout belongs to the starting workspace or was authored directly.
    ///
    /// Files produced later by compilers, generators, archive extraction, or other observed
    /// commands retain their upstream/generated structure instead of being misclassified as new
    /// first-party architecture. A file created through the explicit text-write tool remains
    /// first-party and cannot bypass source-layout policy.
    #[must_use]
    pub fn source_layout_applies(&self, path: &Path) -> bool {
        self.baseline.contains(path) || self.directly_created.contains(path)
    }

    /// Allows exact-file removal only when this product run has a defensible ownership claim.
    pub fn ensure_removable(&self, path: &Path) -> Result<(), DeveloperLoopError> {
        if self.baseline.contains(path)
            || self.directly_created.contains(path)
            || self.command_created.contains(path)
        {
            return Ok(());
        }
        Err(tool(
            "refusing to remove a file that appeared after this product run began; it may be externally produced evidence, so preserve it unless the user starts a new run that explicitly requests its removal",
        ))
    }
}

fn untracked_files(root: &Path) -> Option<BTreeSet<PathBuf>> {
    let output = Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard", "-z"])
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let mut files = BTreeSet::new();
    for encoded in output.stdout.split(|byte| *byte == 0).filter(|path| !path.is_empty()) {
        let Ok(relative) = std::str::from_utf8(encoded) else {
            continue;
        };
        let path = root.join(relative);
        if path.is_file() {
            files.insert(path);
        }
    }
    Some(files)
}

fn regular_files(root: &Path) -> BTreeSet<PathBuf> {
    let mut files = BTreeSet::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(children) = fs::read_dir(directory) else {
            continue;
        };
        for child in children.flatten() {
            let path = child.path();
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            if ignored(relative) {
                continue;
            }
            let Ok(kind) = child.file_type() else {
                continue;
            };
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                files.insert(path);
            }
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_external_file_is_not_owned_by_the_product_run() {
        let workspace = tempfile::tempdir().expect("workspace");
        let baseline = workspace.path().join("baseline.txt");
        fs::write(&baseline, "before").expect("baseline file");
        let mut ownership = WorkspaceOwnership::capture(workspace.path());

        let external = workspace.path().join("api_access.log");
        fs::write(&external, "/projects\n").expect("external evidence");
        assert!(ownership.ensure_removable(&baseline).is_ok());
        assert!(ownership.ensure_removable(&external).is_err());

        let direct = workspace.path().join("draft.txt");
        ownership.record_direct_creation(&direct, false);
        assert!(ownership.ensure_removable(&direct).is_ok());

        let unowned_before = ownership.unowned_files(workspace.path());
        let command_output = workspace.path().join("generated-report.txt");
        fs::write(&command_output, "result\n").expect("command output");
        ownership.record_command_creations(workspace.path(), &unowned_before);
        assert!(ownership.ensure_removable(&command_output).is_ok());
        assert!(ownership.ensure_removable(&external).is_err());

        assert!(ownership.source_layout_applies(&baseline));
        assert!(ownership.source_layout_applies(&direct));
        assert!(!ownership.source_layout_applies(&external));
        assert!(!ownership.source_layout_applies(&command_output));
    }
}
