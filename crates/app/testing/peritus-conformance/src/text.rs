//! Bounded human-readable text admitted to deterministic reports.

use std::error::Error;
use std::fmt;

/// Failure returned when human-readable report text violates its size contract.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReportTextError {
    /// The text contains no bytes.
    Empty,
    /// The UTF-8 representation exceeds [`ReportText::MAX_LENGTH`] bytes.
    TooLong,
}

impl ReportTextError {
    /// Returns the stable diagnostic code for this error.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Empty => "PERITUS-CONFORMANCE-TEXT-001",
            Self::TooLong => "PERITUS-CONFORMANCE-TEXT-002",
        }
    }
}

impl fmt::Display for ReportTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "report text is empty",
            Self::TooLong => "report text is too long",
        })
    }
}

impl Error for ReportTextError {}

/// Validated nonempty UTF-8 text bounded for inclusion in a conformance report.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReportText(String);

impl ReportText {
    /// The maximum UTF-8 representation length in bytes.
    pub const MAX_LENGTH: usize = 4_096;

    /// Validates and stores human-readable report text without truncation.
    ///
    /// # Errors
    ///
    /// Returns [`ReportTextError::Empty`] for empty text and [`ReportTextError::TooLong`] when the
    /// UTF-8 representation exceeds [`Self::MAX_LENGTH`] bytes.
    pub fn new(value: impl Into<String>) -> Result<Self, ReportTextError> {
        let value = value.into();
        validate(&value)?;
        Ok(Self(value))
    }

    /// Borrows the exact validated text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the value and returns the exact validated text.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }

    pub(crate) fn literal(value: &'static str) -> Self {
        assert!(validate(value).is_ok(), "invalid internal report text literal");
        Self(value.to_owned())
    }
}

impl fmt::Display for ReportText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<String> for ReportText {
    type Error = ReportTextError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for ReportText {
    type Error = ReportTextError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

const fn validate(value: &str) -> Result<(), ReportTextError> {
    if value.is_empty() {
        Err(ReportTextError::Empty)
    } else if value.len() > ReportText::MAX_LENGTH {
        Err(ReportTextError::TooLong)
    } else {
        Ok(())
    }
}
