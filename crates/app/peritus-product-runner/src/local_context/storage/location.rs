//! Validate the existing ancestor before creating any run-memory directory.

use super::super::error;
use peritus_agent::DeveloperLoopError;
use std::path::{Component, Path};

pub(super) fn validate(root: &Path, workspace: &Path) -> Result<(), DeveloperLoopError> {
    if !root.is_absolute() || root.components().any(|part| matches!(part, Component::ParentDir)) {
        return Err(error("memory root must be absolute and traversal-free"));
    }
    let workspace = workspace.canonicalize().map_err(|_| error("resolve workspace root"))?;
    let mut ancestor = root;
    let mut suffix = Vec::new();
    while !ancestor.exists() {
        suffix.push(ancestor.file_name().ok_or_else(|| error("invalid memory root ancestor"))?);
        ancestor = ancestor.parent().ok_or_else(|| error("missing memory root ancestor"))?;
    }
    let mut projected =
        ancestor.canonicalize().map_err(|_| error("resolve memory root ancestor"))?;
    for name in suffix.iter().rev() {
        projected.push(name);
    }
    if projected.starts_with(&workspace) || workspace.starts_with(&projected) {
        return Err(error("storage overlaps editable workspace"));
    }
    Ok(())
}
