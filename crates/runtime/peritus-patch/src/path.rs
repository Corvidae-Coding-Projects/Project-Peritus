//! Bounded canonical workspace-relative paths.

use std::{fmt, path::Path};

use crate::{ErrorCode, PatchError, PatchOperationContext, RecoveryClass, RollbackStatus};

/// Maximum UTF-8 bytes in one path component.
pub const MAX_COMPONENT_BYTES: usize = 255;
/// Maximum UTF-8 bytes in one complete workspace-relative path.
pub const MAX_PATH_BYTES: usize = 4_096;
/// Maximum components in one workspace-relative path.
pub const MAX_COMPONENTS: usize = 256;

/// A bounded UTF-8 workspace-relative path in canonical slash-separated form.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkspacePath(String);

impl WorkspacePath {
    /// Validates and stores one canonical workspace-relative path.
    ///
    /// Empty/rooted paths, alternate separators, traversal, control bytes, platform prefixes,
    /// device names, trailing dot/space aliases, and `.git`/`.peritus` components are rejected.
    ///
    /// # Errors
    ///
    /// Returns a stable path or protected-metadata error.
    pub fn new(value: impl Into<String>) -> Result<Self, PatchError> {
        let value = value.into();
        if !crate::verified::path_bounds_valid(value.len(), value.split('/').count())
            || value.starts_with('/')
            || value.ends_with('/')
            || value.bytes().any(forbidden_byte)
        {
            return Err(invalid_path());
        }
        for component in value.split('/') {
            if !valid_component(component) {
                return Err(invalid_path());
            }
            if protected_component(component) {
                return Err(PatchError::message(
                    ErrorCode::ProtectedPath,
                    RecoveryClass::CorrectPatch,
                    PatchOperationContext::ValidatePath,
                    RollbackStatus::NotRequired,
                    "path names protected workspace metadata",
                ));
            }
        }
        Ok(Self(value))
    }

    /// Borrows the canonical slash-separated UTF-8 value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Borrows the value as a relative platform path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }

    pub(crate) fn components(&self) -> std::str::Split<'_, char> {
        self.0.split('/')
    }

    pub(crate) fn is_ancestor_of(&self, other: &Self) -> bool {
        other.0.strip_prefix(&self.0).is_some_and(|suffix| suffix.starts_with('/'))
    }
}

impl fmt::Display for WorkspacePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

const fn forbidden_byte(byte: u8) -> bool {
    byte == 0 || byte < 0x20 || byte == 0x7f || matches!(byte, b'\\' | b':')
}

fn valid_component(component: &str) -> bool {
    !component.is_empty()
        && component != "."
        && component != ".."
        && component.len() <= MAX_COMPONENT_BYTES
        && !component.ends_with(['.', ' '])
        && !windows_device_name(component)
}

fn protected_component(component: &str) -> bool {
    component.eq_ignore_ascii_case(".git")
        || component.eq_ignore_ascii_case(".peritus")
        || component.to_ascii_lowercase().starts_with(".peritus-txn-")
}

fn windows_device_name(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or(component);
    let uppercase = stem.to_ascii_uppercase();
    matches!(uppercase.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
        || (uppercase.len() == 4
            && (uppercase.starts_with("COM") || uppercase.starts_with("LPT"))
            && matches!(uppercase.as_bytes()[3], b'1'..=b'9'))
}

const fn invalid_path() -> PatchError {
    PatchError::message(
        ErrorCode::InvalidPath,
        RecoveryClass::CorrectPatch,
        PatchOperationContext::ValidatePath,
        RollbackStatus::NotRequired,
        "path is not a canonical bounded workspace-relative path",
    )
}

#[cfg(test)]
mod tests {
    use super::WorkspacePath;

    #[test]
    fn accepts_normal_unicode_and_leading_dash_paths() {
        for value in ["src/lib.rs", "docs/naïve.md", "-fixture/file"] {
            assert_eq!(WorkspacePath::new(value).expect("valid path").as_str(), value);
        }
    }

    #[test]
    fn rejects_traversal_aliases_devices_and_metadata() {
        for value in [
            "",
            "/etc/passwd",
            "a/../b",
            "a//b",
            "a\\b",
            "C:/x",
            "name.",
            "NUL",
            ".git/config",
            "nested/.GIT/index",
            ".peritus/state",
            "a\0b",
        ] {
            assert!(WorkspacePath::new(value).is_err(), "accepted {value:?}");
        }
    }
}
