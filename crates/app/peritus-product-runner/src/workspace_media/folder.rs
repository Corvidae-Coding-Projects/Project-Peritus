//! Explicit-path-only media for direct folders: no recursive discovery or private-state reads.

use super::{ProductRunnerError, ProviderProfile, WorkspaceImages, attach, supported_extension};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub fn discover_explicit(
    root: &Path,
    task: &str,
    profile: &ProviderProfile,
    protected: &[PathBuf],
) -> Result<WorkspaceImages, ProductRunnerError> {
    let paths = task
        .split_whitespace()
        .chain(task.split(['`', '"', '\n']))
        .filter_map(|token| {
            let token = token.trim().trim_matches(['`', '"', '\'', '(', ')', ',', ';']);
            let path = Path::new(token);
            if !supported_extension(path) {
                return None;
            }
            let relative = if path.is_absolute() { path.strip_prefix(root).ok()? } else { path };
            crate::developer_tools::checked_protected_file(
                root,
                relative.to_str()?,
                task,
                protected,
            )
            .ok()
        })
        .filter(|path| path.is_file())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    attach(root, paths, Vec::new(), profile, super::requests_visual_inspection(task))
}
