//! Validated stable identifiers used by conformance definitions and reports.

use std::error::Error;
use std::fmt;

const MAX_IDENTIFIER_LENGTH: usize = 128;

/// Failure returned when a suite, case, or observation identifier is invalid.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IdentifierError {
    /// The identifier contains no bytes.
    Empty,
    /// The identifier exceeds the documented byte limit.
    TooLong,
    /// A dot-separated segment is empty.
    EmptySegment,
    /// A segment does not start with an ASCII lowercase letter.
    InvalidSegmentStart,
    /// A byte is outside ASCII lowercase letters, digits, hyphens, and dots.
    InvalidCharacter,
}

impl IdentifierError {
    /// Returns the stable diagnostic code for this error.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Empty => "PERITUS-CONFORMANCE-ID-001",
            Self::TooLong => "PERITUS-CONFORMANCE-ID-002",
            Self::EmptySegment => "PERITUS-CONFORMANCE-ID-003",
            Self::InvalidSegmentStart => "PERITUS-CONFORMANCE-ID-004",
            Self::InvalidCharacter => "PERITUS-CONFORMANCE-ID-005",
        }
    }
}

impl fmt::Display for IdentifierError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "identifier is empty",
            Self::TooLong => "identifier is too long",
            Self::EmptySegment => "identifier contains an empty segment",
            Self::InvalidSegmentStart => "identifier segment has an invalid first byte",
            Self::InvalidCharacter => "identifier contains an invalid byte",
        })
    }
}

impl Error for IdentifierError {}

/// Failure returned when a conformance failure code is invalid.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FailureCodeError {
    /// The code contains no bytes.
    Empty,
    /// The code exceeds the documented byte limit.
    TooLong,
    /// The code does not begin with an ASCII uppercase letter.
    InvalidStart,
    /// The code contains an unsupported byte or malformed separator.
    InvalidCharacter,
}

impl FailureCodeError {
    /// Returns the stable diagnostic code for this error.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Empty => "PERITUS-CONFORMANCE-CODE-001",
            Self::TooLong => "PERITUS-CONFORMANCE-CODE-002",
            Self::InvalidStart => "PERITUS-CONFORMANCE-CODE-003",
            Self::InvalidCharacter => "PERITUS-CONFORMANCE-CODE-004",
        }
    }
}

impl fmt::Display for FailureCodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "failure code is empty",
            Self::TooLong => "failure code is too long",
            Self::InvalidStart => "failure code has an invalid first byte",
            Self::InvalidCharacter => "failure code contains an invalid byte",
        })
    }
}

impl Error for FailureCodeError {}

/// Identifies one conformance suite.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SuiteId(String);

impl SuiteId {
    /// The maximum encoded length in bytes.
    pub const MAX_LENGTH: usize = MAX_IDENTIFIER_LENGTH;

    /// Validates and stores a stable identifier.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError`] when the value is empty, too long, or malformed.
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_path_identifier(&value)?;
        Ok(Self(value))
    }

    /// Borrows the exact validated identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn catalog(value: &'static str) -> Self {
        assert!(validate_path_identifier(value).is_ok(), "invalid standard suite identifier");
        Self(value.to_owned())
    }
}

impl fmt::Display for SuiteId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Identifies one case within a conformance suite.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CaseId(String);

impl CaseId {
    /// The maximum encoded length in bytes.
    pub const MAX_LENGTH: usize = MAX_IDENTIFIER_LENGTH;

    /// Validates and stores a stable identifier.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError`] when the value is empty, too long, or malformed.
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_path_identifier(&value)?;
        Ok(Self(value))
    }

    /// Borrows the exact validated identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn catalog(value: &'static str) -> Self {
        assert!(validate_path_identifier(value).is_ok(), "invalid standard case identifier");
        Self(value.to_owned())
    }
}

impl fmt::Display for CaseId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Identifies one typed observation within a case report.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ObservationId(String);

impl ObservationId {
    /// The maximum encoded length in bytes.
    pub const MAX_LENGTH: usize = MAX_IDENTIFIER_LENGTH;

    /// Validates and stores a stable identifier.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError`] when the value is empty, too long, or malformed.
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_path_identifier(&value)?;
        Ok(Self(value))
    }

    /// Borrows the exact validated identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn catalog(value: &'static str) -> Self {
        assert!(validate_path_identifier(value).is_ok(), "invalid standard observation identifier");
        Self(value.to_owned())
    }
}

impl fmt::Display for ObservationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A stable machine-readable failure category.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FailureCode(String);

impl FailureCode {
    /// The maximum encoded length in bytes.
    pub const MAX_LENGTH: usize = MAX_IDENTIFIER_LENGTH;

    /// Validates and stores a failure code.
    ///
    /// # Errors
    ///
    /// Returns [`FailureCodeError`] unless the code starts with an ASCII uppercase letter and
    /// contains only uppercase letters, digits, and single hyphen separators.
    pub fn new(value: impl Into<String>) -> Result<Self, FailureCodeError> {
        let value = value.into();
        validate_failure_code(&value)?;
        Ok(Self(value))
    }

    /// Borrows the exact validated code.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn catalog(value: &'static str) -> Self {
        assert!(validate_failure_code(value).is_ok(), "invalid standard failure code");
        Self(value.to_owned())
    }
}

impl fmt::Display for FailureCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

fn validate_path_identifier(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::Empty);
    }
    if value.len() > MAX_IDENTIFIER_LENGTH {
        return Err(IdentifierError::TooLong);
    }
    for segment in value.split('.') {
        if segment.is_empty() {
            return Err(IdentifierError::EmptySegment);
        }
        let mut bytes = segment.bytes();
        if !bytes.next().is_some_and(|byte| byte.is_ascii_lowercase()) {
            return Err(IdentifierError::InvalidSegmentStart);
        }
        if bytes.any(|byte| !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')) {
            return Err(IdentifierError::InvalidCharacter);
        }
    }
    Ok(())
}

fn validate_failure_code(value: &str) -> Result<(), FailureCodeError> {
    if value.is_empty() {
        return Err(FailureCodeError::Empty);
    }
    if value.len() > MAX_IDENTIFIER_LENGTH {
        return Err(FailureCodeError::TooLong);
    }
    let bytes = value.as_bytes();
    if !bytes[0].is_ascii_uppercase() {
        return Err(FailureCodeError::InvalidStart);
    }
    if bytes.last() == Some(&b'-')
        || bytes.windows(2).any(|pair| pair == b"--")
        || bytes
            .iter()
            .any(|byte| !(byte.is_ascii_uppercase() || byte.is_ascii_digit() || *byte == b'-'))
    {
        return Err(FailureCodeError::InvalidCharacter);
    }
    Ok(())
}
