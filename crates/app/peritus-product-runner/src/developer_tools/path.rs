//! Workspace-relative path validation without ambient traversal.

use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use peritus_agent::DeveloperLoopError;

pub fn checked(
    root: &Path,
    value: &str,
    allow_missing: bool,
) -> Result<PathBuf, DeveloperLoopError> {
    let relative = Path::new(value);
    if relative.is_absolute()
        || relative.components().any(|component| !matches!(component, Component::Normal(_)))
        || relative.starts_with(".git")
    {
        return Err(tool("path must be a normal workspace-relative path"));
    }
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err(tool("invalid path component"));
        };
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(tool("symbolic links are not developer tool targets"));
            }
            Ok(_) => {}
            Err(error) if allow_missing && error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(tool(error.to_string())),
        }
    }
    Ok(current)
}

pub fn tool(detail: impl Into<String>) -> DeveloperLoopError {
    DeveloperLoopError::Tool(detail.into())
}
