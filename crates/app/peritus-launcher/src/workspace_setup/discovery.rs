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
    repository: GitRepository,
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
        let repository = GitRepository::open(
            RepositoryOptions::new(path).allow_discovery_from_descendant(true),
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
        Ok(Self { repository, root_text, identity_text })
    }

    /// Borrows the structured repository adapter.
    #[must_use]
    pub const fn repository(&self) -> &GitRepository {
        &self.repository
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
