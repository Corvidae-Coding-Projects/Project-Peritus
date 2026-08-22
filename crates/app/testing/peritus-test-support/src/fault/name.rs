//! Validated fault point and behavior names.

use super::FaultNameError;

const MAX_FAULT_NAME_LENGTH: usize = 128;

/// A stable location at which a test adapter checks for a fault.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FaultPoint(String);

impl FaultPoint {
    /// Validates and stores a stable hierarchical point name.
    ///
    /// # Errors
    ///
    /// Returns [`FaultNameError`] when the name is empty, too long, or malformed.
    pub fn new(value: impl Into<String>) -> Result<Self, FaultNameError> {
        let value = value.into();
        validate_name(&value)?;
        Ok(Self(value))
    }

    /// Borrows the validated name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A caller-interpreted label for one injected fault behavior.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FaultLabel(String);

impl FaultLabel {
    /// Validates and stores a stable hierarchical behavior label.
    ///
    /// # Errors
    ///
    /// Returns [`FaultNameError`] when the name is empty, too long, or malformed.
    pub fn new(value: impl Into<String>) -> Result<Self, FaultNameError> {
        let value = value.into();
        validate_name(&value)?;
        Ok(Self(value))
    }

    /// Borrows the validated name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn validate_name(value: &str) -> Result<(), FaultNameError> {
    if value.is_empty() {
        return Err(FaultNameError::Empty);
    }
    if value.len() > MAX_FAULT_NAME_LENGTH {
        return Err(FaultNameError::TooLong);
    }
    let mut at_start = true;
    for byte in value.bytes() {
        if byte == b'.' {
            if at_start {
                return Err(FaultNameError::EmptySegment);
            }
            at_start = true;
        } else if at_start {
            if !byte.is_ascii_lowercase() {
                return Err(FaultNameError::InvalidSegmentStart);
            }
            at_start = false;
        } else if !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-') {
            return Err(FaultNameError::InvalidCharacter);
        }
    }
    if at_start { Err(FaultNameError::EmptySegment) } else { Ok(()) }
}
