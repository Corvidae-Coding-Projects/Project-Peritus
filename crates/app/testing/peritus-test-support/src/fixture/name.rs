//! Portable fixture identity components.

use super::{FixtureError, FixtureErrorKind};

const MAX_COMPONENT_LENGTH: usize = 64;

/// A portable lowercase fixture surface or case name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FixtureName(String);

impl FixtureName {
    /// Validates `[a-z0-9][a-z0-9-]*` within 64 bytes.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureErrorKind::InvalidName`] for invalid input.
    pub fn new(value: impl Into<String>) -> Result<Self, FixtureError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= MAX_COMPONENT_LENGTH
            && value.bytes().enumerate().all(|(index, byte)| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || (index > 0 && byte == b'-')
            });
        if valid {
            Ok(Self(value))
        } else {
            Err(FixtureError::new(
                FixtureErrorKind::InvalidName,
                format!("invalid fixture name {value:?}"),
            ))
        }
    }

    /// Borrows the validated name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An opaque, portable surface-version directory component.
///
/// This type validates storage syntax only and deliberately assigns no semantic-version meaning.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FixtureVersion(String);

impl FixtureVersion {
    /// Validates a nonempty ASCII alphanumeric, dot, underscore, and hyphen component.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureErrorKind::InvalidName`] for invalid input.
    pub fn new(value: impl Into<String>) -> Result<Self, FixtureError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= MAX_COMPONENT_LENGTH
            && value.bytes().enumerate().all(|(index, byte)| {
                byte.is_ascii_alphanumeric() || (index > 0 && matches!(byte, b'.' | b'_' | b'-'))
            });
        if valid {
            Ok(Self(value))
        } else {
            Err(FixtureError::new(
                FixtureErrorKind::InvalidName,
                format!("invalid fixture version {value:?}"),
            ))
        }
    }

    /// Borrows the validated opaque version.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
