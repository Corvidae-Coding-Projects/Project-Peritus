//! Authority-safe UTF-8 relative paths with host-native component validation.

use std::{fmt, path::Path};

use crate::{ErrorCode, PatchError, PatchOperationContext, RecoveryClass, RollbackStatus};

/// Historical portable component capacity, used only to classify legacy representations.
pub const MAX_COMPONENT_BYTES: usize = 255;
/// Historical portable path capacity, used only to classify legacy representations.
pub const MAX_PATH_BYTES: usize = 4_096;
/// Historical portable depth, used only to classify legacy representations.
pub const MAX_COMPONENTS: usize = 256;

/// A UTF-8 workspace-relative path in canonical slash-separated form.
/// Host filesystem capacity is checked by filesystem operations, not this representation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkspacePath(String);

impl WorkspacePath {
    /// Validates and stores one canonical workspace-relative path.
    ///
    /// Empty/rooted paths, traversal, NUL and protected metadata are rejected on every host.
    /// Windows separators, prefixes, device names and aliases are rejected on Windows;
    /// Unix names do not inherit those restrictions. This type does not grant effect authority.
    ///
    /// # Errors
    ///
    /// Returns a stable path or protected-metadata error.
    pub fn new(value: impl Into<String>) -> Result<Self, PatchError> {
        let value = value.into();
        if !crate::verified::path_bounds_valid(value.len(), value.split('/').count())
            || value.starts_with('/')
            || value.ends_with('/')
            || value.bytes().any(native_forbidden_byte)
        {
            return Err(invalid_path());
        }
        for component in value.split('/') {
            if !native_component(component) {
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

    pub(crate) fn is_legacy_portable(&self) -> bool {
        self.0.len() <= MAX_PATH_BYTES
            && self.components().count() <= MAX_COMPONENTS
            && !self.0.bytes().any(portable_forbidden_byte)
            && self.components().all(|component| {
                component.len() <= MAX_COMPONENT_BYTES && portable_component(component)
            })
    }

    /// Returns whether this host-native path needs an extended persistence representation.
    /// Historical portable capacities classify old encodings; they do not limit new paths.
    #[must_use]
    pub fn requires_extended_encoding(&self) -> bool {
        !self.is_legacy_portable()
    }
}

impl fmt::Display for WorkspacePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

const fn portable_forbidden_byte(byte: u8) -> bool {
    byte == 0 || byte < 0x20 || byte == 0x7f || matches!(byte, b'\\' | b':')
}

fn native_component(component: &str) -> bool {
    #[cfg(unix)]
    let native = true;
    #[cfg(not(unix))]
    let native = portable_component(component);
    !component.is_empty() && component != "." && component != ".." && native
}

const fn native_forbidden_byte(byte: u8) -> bool {
    #[cfg(unix)]
    let forbidden = byte == 0;
    #[cfg(not(unix))]
    let forbidden = portable_forbidden_byte(byte);
    forbidden
}

fn portable_component(component: &str) -> bool {
    !component.is_empty() && !component.ends_with(['.', ' ']) && !windows_device_name(component)
}

/// Tags the host interpretation of newly accepted native paths in internal versioned records.
pub const fn native_platform_tag() -> u8 {
    #[cfg(unix)]
    let tag = 1;
    #[cfg(windows)]
    let tag = 2;
    #[cfg(not(any(unix, windows)))]
    let tag = 3;
    tag
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
        "path is not an authority-safe native workspace-relative path",
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
            ".git/config",
            "nested/.GIT/index",
            ".peritus/state",
            "a\0b",
        ] {
            assert!(WorkspacePath::new(value).is_err(), "accepted {value:?}");
        }
        #[cfg(not(unix))]
        for value in ["a\\b", "C:/x", "name.", "NUL"] {
            assert!(WorkspacePath::new(value).is_err(), "accepted {value:?}");
        }
    }
}
