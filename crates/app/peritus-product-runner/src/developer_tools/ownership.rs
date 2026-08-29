//! Workspace file ownership retained across one complete product run.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use peritus_agent::DeveloperLoopError;

use super::path::{ignored, tool};

/// Distinguishes invocation-baseline files and direct model writes from late external evidence.
#[derive(Clone)]
pub struct WorkspaceOwnership {
    baseline_files: BTreeSet<PathBuf>,
    directly_created_files: BTreeSet<PathBuf>,
}

impl WorkspaceOwnership {
    /// Captures regular files already present when the product run begins.
    #[must_use]
    pub fn capture(root: &Path) -> Self {
        let mut baseline_files = BTreeSet::new();
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
                    baseline_files.insert(path);
                }
            }
        }
        Self { baseline_files, directly_created_files: BTreeSet::new() }
    }

    /// Records a new file created through the explicit text-write tool.
    pub fn record_direct_creation(&mut self, path: &Path, existed_before: bool) {
        if !existed_before {
            self.directly_created_files.insert(path.to_path_buf());
        }
    }

    /// Allows exact-file removal only when this product run has a defensible ownership claim.
    pub fn ensure_removable(&self, path: &Path) -> Result<(), DeveloperLoopError> {
        if self.baseline_files.contains(path) || self.directly_created_files.contains(path) {
            return Ok(());
        }
        Err(tool(
            "refusing to remove a file that appeared after this product run began; it may be externally produced evidence, so preserve it unless the user starts a new run that explicitly requests its removal",
        ))
    }
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
    }
}
