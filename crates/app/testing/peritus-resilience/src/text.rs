//! Bounded human-readable report text.

use std::error::Error;
use std::fmt;

/// Maximum UTF-8 bytes in one report text value.
pub const MAX_QUALIFICATION_TEXT_BYTES: usize = 1_024;

/// Failure to validate human-readable qualification text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextError {
    /// Text was empty or only whitespace.
    Empty,
    /// Text exceeded [`MAX_QUALIFICATION_TEXT_BYTES`].
    TooLong {
        /// Actual UTF-8 byte length.
        actual_bytes: usize,
    },
    /// Text contained a control character.
    ControlCharacter,
}

impl fmt::Display for TextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("qualification text must not be empty"),
            Self::TooLong { actual_bytes } => write!(
                formatter,
                "qualification text is {actual_bytes} bytes; maximum is {MAX_QUALIFICATION_TEXT_BYTES}"
            ),
            Self::ControlCharacter => {
                formatter.write_str("qualification text contains an ASCII control character")
            }
        }
    }
}

impl Error for TextError {}

/// Validated nonempty report text with a fixed UTF-8 byte bound.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct QualificationText(String);

impl QualificationText {
    /// Validates and owns report text.
    ///
    /// # Errors
    ///
    /// Returns [`TextError::Empty`] for empty or whitespace-only text,
    /// [`TextError::TooLong`] when the UTF-8 representation exceeds
    /// [`MAX_QUALIFICATION_TEXT_BYTES`], and [`TextError::ControlCharacter`] when a control
    /// character is present.
    pub fn new(value: impl Into<String>) -> Result<Self, TextError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(TextError::Empty);
        }
        if value.len() > MAX_QUALIFICATION_TEXT_BYTES {
            return Err(TextError::TooLong { actual_bytes: value.len() });
        }
        if value.chars().any(char::is_control) {
            return Err(TextError::ControlCharacter);
        }
        Ok(Self(value))
    }

    /// Returns the validated text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for QualificationText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
