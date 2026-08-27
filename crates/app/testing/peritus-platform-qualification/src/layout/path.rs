//! Target-independent validation for target-native absolute install paths.

use crate::{Platform, QualificationError};

use super::layout_error;

/// Validated target-native absolute path represented with forward separators in H2 evidence.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InstallPath(String);

impl InstallPath {
    /// Validates a target-native absolute path without consulting the current host.
    ///
    /// # Errors
    ///
    /// Rejects relative, traversal-bearing, control-bearing, or noncanonical paths.
    pub fn new(platform: Platform, value: impl Into<String>) -> Result<Self, QualificationError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 4_096
            || value.bytes().any(|byte| byte == 0 || byte.is_ascii_control())
            || value.contains('\\')
            || value.ends_with('/')
        {
            return Err(layout_error("install path is not canonical"));
        }
        let absolute = match platform {
            Platform::Linux | Platform::Macos => value.starts_with('/'),
            Platform::Windows => {
                value.len() >= 3
                    && value.as_bytes()[0].is_ascii_alphabetic()
                    && value.as_bytes()[1] == b':'
                    && value.as_bytes()[2] == b'/'
            }
        };
        if !absolute {
            return Err(layout_error("install path must be absolute for its target"));
        }
        let components = match platform {
            Platform::Linux | Platform::Macos => &value[1..],
            Platform::Windows => &value[3..],
        };
        if components
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."))
        {
            return Err(layout_error("install path contains a noncanonical component"));
        }
        Ok(Self(value))
    }

    /// Borrows the canonical slash-separated path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Creates a validated descendant path.
    ///
    /// # Errors
    ///
    /// Rejects a noncanonical relative suffix.
    pub fn join(&self, platform: Platform, suffix: &str) -> Result<Self, QualificationError> {
        if suffix.is_empty()
            || suffix.starts_with('/')
            || suffix.contains('\\')
            || suffix
                .split('/')
                .any(|component| component.is_empty() || matches!(component, "." | ".."))
        {
            return Err(layout_error("install path suffix is not canonical"));
        }
        Self::new(platform, format!("{}/{suffix}", self.0))
    }

    /// Reports whether this path is a strict descendant of another canonical path.
    #[must_use]
    pub fn is_beneath(&self, parent: &Self) -> bool {
        self.0
            .strip_prefix(&parent.0)
            .is_some_and(|suffix| suffix.starts_with('/') && suffix.len() > 1)
    }
}
