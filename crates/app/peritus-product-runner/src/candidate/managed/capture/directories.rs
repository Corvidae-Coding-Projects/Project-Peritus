//! Inspect ordinary directories still represented by a gitlink in the real index.

use super::{ManagedBaseline, failure, private_git, validate_path, workspace_filter};
use crate::ProductRunnerError;
use std::{collections::BTreeSet, fs, path::Path};

pub(super) fn expand(
    root: &Path,
    prior: Option<&ManagedBaseline>,
    paths: &mut BTreeSet<String>,
) -> Result<(), ProductRunnerError> {
    let directories = paths
        .iter()
        .filter_map(|path| match ordinary_directory(root, path) {
            Ok(true) => Some(Ok(path.clone())),
            Ok(false) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if directories.is_empty() {
        return Ok(());
    }
    // The real index hides everything below a gitlink even after its .git is removed.
    // An empty private index lets Git apply ignore rules without that stale boundary.
    let temporary = tempfile::tempdir().map_err(failure)?;
    let index = temporary.path().join("index");
    private_git(root, &index, &["read-tree", "--empty"], None)?;
    for directory in directories {
        let output = private_git(
            root,
            &index,
            &["ls-files", "-z", "--others", "--exclude-standard", "--", &directory],
            None,
        )?;
        for path in output.split(|byte| *byte == 0).filter(|path| !path.is_empty()) {
            let path = std::str::from_utf8(path).map_err(failure)?.trim_end_matches('/');
            if !workspace_filter::generated(Path::new(path)) {
                paths.insert(path.to_owned());
            }
        }
        // Previously owned files stay owned even if the task adds an ignore rule.
        if let Some(child) = prior.and_then(|baseline| baseline.nested.get(&directory)) {
            let mut owned = BTreeSet::new();
            child.append_paths(Path::new(&directory), &mut owned);
            for path in owned {
                paths.insert(super::super::git_path::tree_name(&path)?);
            }
        }
    }
    Ok(())
}

fn ordinary_directory(root: &Path, path: &str) -> Result<bool, ProductRunnerError> {
    validate_path(Path::new(path))?;
    if !super::super::paths::parent_is_directory(root, Path::new(path))? {
        return Ok(false);
    }
    let absolute = root.join(path);
    match fs::symlink_metadata(&absolute) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(failure(error)),
    }
    match fs::symlink_metadata(absolute.join(".git")) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(failure(error)),
    }
}
