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
    if relative == Path::new(".") {
        return Ok(root.to_path_buf());
    }
    if relative.is_absolute()
        || relative.components().any(|component| !matches!(component, Component::Normal(_)))
        || protected_metadata(relative)
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

pub fn canonical_command_cwd(root: &Path, cwd: &Path) -> Result<PathBuf, DeveloperLoopError> {
    let cwd = cwd
        .canonicalize()
        .map_err(|error| tool(format!("open command working directory: {error}")))?;
    if !cwd.starts_with(root) {
        return Err(tool("command working directory escaped the managed workspace"));
    }
    Ok(cwd)
}

pub fn tool(detail: impl Into<String>) -> DeveloperLoopError {
    DeveloperLoopError::Tool(detail.into())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraversalExclusion {
    ProtectedMetadata,
    DefaultCache,
}

impl TraversalExclusion {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::ProtectedMetadata => "protected_metadata",
            Self::DefaultCache => "default_cache",
        }
    }

    pub const fn detail(self) -> &'static str {
        match self {
            Self::ProtectedMetadata => {
                "repository control metadata is never exposed as an ordinary workspace target"
            }
            Self::DefaultCache => {
                "a common generated cache is omitted from broad traversal; target this path explicitly to inspect it"
            }
        }
    }

    pub const fn targetable(self) -> bool {
        matches!(self, Self::DefaultCache)
    }
}

pub fn traversal_exclusion(path: &Path, targeted_root: Option<&Path>) -> Option<TraversalExclusion> {
    if protected_metadata(path) {
        return Some(TraversalExclusion::ProtectedMetadata);
    }
    if targets_default_exclusion(targeted_root)
        && targeted_root.is_some_and(|root| path.starts_with(root))
    {
        return None;
    }
    contains_default_exclusion(path).then_some(TraversalExclusion::DefaultCache)
}

pub fn protected_metadata(path: &Path) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(".git"))
    })
}

pub fn targets_default_exclusion(path: Option<&Path>) -> bool {
    path.is_some_and(contains_default_exclusion)
}

fn contains_default_exclusion(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component.as_os_str().to_str(),
            Some("target" | "node_modules" | ".venv" | "__pycache__")
        )
    })
}
