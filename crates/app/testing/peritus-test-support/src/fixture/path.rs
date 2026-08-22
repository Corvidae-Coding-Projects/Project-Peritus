//! Strict portable relative paths inside fixtures and temporary repositories.

use super::{FixtureError, FixtureErrorKind};
use std::path::Path;

const MAX_FIXTURE_PATH_LENGTH: usize = 240;

/// A portable, contained, slash-separated relative fixture path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FixturePath(String);

impl FixturePath {
    /// Validates a portable ASCII relative path.
    ///
    /// Segments use ASCII letters, digits, dots, underscores, and hyphens. Empty, dot-only,
    /// trailing-dot, Windows-device, rooted, prefixed, and backslash-separated paths are rejected.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureErrorKind::InvalidPath`] for invalid input.
    pub fn new(value: impl Into<String>) -> Result<Self, FixtureError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= MAX_FIXTURE_PATH_LENGTH
            && !value.starts_with('/')
            && !value.contains('\\')
            && value.split('/').all(valid_segment);
        if valid {
            Ok(Self(value))
        } else {
            Err(FixtureError::new(
                FixtureErrorKind::InvalidPath,
                format!("invalid portable fixture path {value:?}"),
            ))
        }
    }

    /// Borrows the normalized slash-separated representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Borrows this value as a platform path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }
}

fn valid_segment(segment: &str) -> bool {
    segment != "."
        && segment != ".."
        && !segment.is_empty()
        && !segment.ends_with('.')
        && segment.bytes().any(|byte| byte.is_ascii_alphanumeric())
        && segment
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        && !windows_device_name(segment)
}

fn windows_device_name(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or(segment);
    let uppercase = stem.to_ascii_uppercase();
    matches!(uppercase.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (uppercase.len() == 4
            && (uppercase.starts_with("COM") || uppercase.starts_with("LPT"))
            && matches!(uppercase.as_bytes()[3], b'1'..=b'9'))
}
