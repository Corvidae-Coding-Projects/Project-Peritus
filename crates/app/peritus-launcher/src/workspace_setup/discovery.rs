//! Friendly path handling over the structured C1 Git discovery boundary.

use std::{
    env,
    path::{Path, PathBuf},
};

use peritus_git::{GitRepository, RepositoryOptions};

use crate::LauncherError;

/// One canonical repository selected by the user or current directory.
#[derive(Clone, Debug)]
pub struct DiscoveredRepository {
    repository: Option<GitRepository>,
    root_text: String,
    identity_text: String,
}

impl DiscoveredRepository {
    /// Discovers a repository from a directory or any of its descendants.
    ///
    /// # Errors
    ///
    /// Returns the structured Git discovery failure or a non-UTF-8 path failure.
    pub fn open(path: &Path) -> Result<Self, LauncherError> {
        let path = path
            .canonicalize()
            .map_err(|error| LauncherError::filesystem("open workspace directory", path, error))?;
        if !path.is_dir() {
            return Err(LauncherError::WorkspaceSetup(
                "workspace path must be a directory".to_owned(),
            ));
        }
        if !has_git_parent(&path)? {
            return Self::folder(&path);
        }
        let repository = GitRepository::open(
            RepositoryOptions::new(&path).allow_discovery_from_descendant(true),
        )?;
        let root_text = repository
            .identity()
            .repository_root()
            .to_str()
            .ok_or_else(|| {
                LauncherError::WorkspaceSetup(
                    "the repository path is not representable as UTF-8".to_owned(),
                )
            })?
            .to_owned();
        let identity_text = hex(repository.identity().digest().as_bytes());
        Ok(Self { repository: Some(repository), root_text, identity_text })
    }

    /// Observes one direct folder without invoking Git or enumerating its children.
    pub fn folder(path: &Path) -> Result<Self, LauncherError> {
        let folder = peritus_workspace::FolderIdentity::observe(path)
            .map_err(|error| LauncherError::filesystem("open folder", path, error))?;
        let root_text = folder
            .root()
            .to_str()
            .ok_or_else(|| LauncherError::WorkspaceSetup("workspace path is not UTF-8".to_owned()))?
            .to_owned();
        Ok(Self { repository: None, root_text, identity_text: hex(folder.digest().as_bytes()) })
    }

    /// Borrows the structured repository adapter.
    #[must_use]
    pub const fn repository(&self) -> Option<&GitRepository> {
        self.repository.as_ref()
    }

    /// Borrows the canonical source-repository root for display and durable state.
    #[must_use]
    pub fn root_text(&self) -> &str {
        &self.root_text
    }

    /// Borrows the exact repository-identity digest in canonical lowercase hex.
    #[must_use]
    pub fn identity_text(&self) -> &str {
        &self.identity_text
    }
}

fn has_git_parent(path: &Path) -> Result<bool, LauncherError> {
    #[cfg(unix)]
    let device = {
        use std::os::unix::fs::MetadataExt as _;
        path.metadata()
            .map_err(|error| {
                LauncherError::filesystem("inspect workspace filesystem", path, error)
            })?
            .dev()
    };
    for ancestor in path.ancestors() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            let metadata = ancestor.metadata().map_err(|error| {
                LauncherError::filesystem("inspect workspace ancestor", ancestor, error)
            })?;
            // Match ordinary Git discovery: do not adopt an unrelated repository across a mount.
            if metadata.dev() != device {
                break;
            }
        }
        let marker = ancestor.join(".git");
        if marker.is_file()
            || (marker.join("HEAD").is_file()
                && (marker.join("objects").is_dir() || marker.join("commondir").is_file()))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Expands one user-entered path without requiring shell expansion or exports.
pub fn user_path(input: &str) -> Result<PathBuf, LauncherError> {
    let trimmed = input.trim().trim_matches(|character| matches!(character, '\'' | '"'));
    if trimmed.is_empty() {
        return Err(LauncherError::WorkspaceSetup("a repository path is required".to_owned()));
    }
    if trimmed == "~" || trimmed.starts_with("~/") || trimmed.starts_with("~\\") {
        let home = env::var_os("HOME").ok_or_else(|| {
            LauncherError::WorkspaceSetup(
                "the home directory is unavailable; enter an absolute path".to_owned(),
            )
        })?;
        let remainder = trimmed.trim_start_matches('~').trim_start_matches(['/', '\\']);
        return Ok(PathBuf::from(home).join(remainder));
    }
    Ok(PathBuf::from(trimmed))
}

pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_path_accepts_quotes_and_relative_paths() {
        assert_eq!(user_path("  './my repo'  ").expect("path"), PathBuf::from("./my repo"));
    }

    #[test]
    fn hex_is_lowercase_and_exact() {
        assert_eq!(hex(&[0, 15, 16, 255]), "000f10ff");
    }
}
