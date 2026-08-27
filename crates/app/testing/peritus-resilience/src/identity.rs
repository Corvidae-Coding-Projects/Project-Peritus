//! Validated identifiers and evidence digests.

use std::error::Error;
use std::fmt;

const MAX_ID_BYTES: usize = 96;

/// The reason an identifier could not be admitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueViolation {
    /// The identifier was empty.
    Empty,
    /// The identifier exceeded the documented byte bound.
    TooLong,
    /// The identifier contained a byte outside the stable identifier alphabet.
    InvalidCharacter,
    /// The first byte was not an ASCII lowercase letter or digit.
    InvalidStart,
}

/// Validation failure for a typed identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValueError {
    violation: ValueViolation,
    actual_bytes: usize,
}

impl ValueError {
    const fn new(violation: ValueViolation, actual_bytes: usize) -> Self {
        Self { violation, actual_bytes }
    }

    /// Returns the rejected invariant.
    #[must_use]
    pub const fn violation(self) -> ValueViolation {
        self.violation
    }

    /// Returns the input length without retaining the rejected value.
    #[must_use]
    pub const fn actual_bytes(self) -> usize {
        self.actual_bytes
    }
}

impl fmt::Display for ValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid bounded identifier: {:?} ({} bytes)",
            self.violation, self.actual_bytes
        )
    }
}

impl Error for ValueError {}

/// Stable identifier for one resilience scenario.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ScenarioId(String);

impl ScenarioId {
    /// Creates a scenario identifier.
    ///
    /// # Errors
    ///
    /// Returns `ValueError` when the value violates the stable identifier contract.
    pub fn new(value: impl Into<String>) -> Result<Self, ValueError> {
        validate(value.into()).map(Self)
    }

    /// Returns the text of the scenario identifier.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for ScenarioId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Stable identifier for the implementation under qualification.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SubjectId(String);

impl SubjectId {
    /// Creates a subject identifier.
    ///
    /// # Errors
    ///
    /// Returns `ValueError` when the value violates the stable identifier contract.
    pub fn new(value: impl Into<String>) -> Result<Self, ValueError> {
        validate(value.into()).map(Self)
    }

    /// Returns the text of the subject identifier.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for SubjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Stable identifier for one external evidence record.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvidenceId(String);

impl EvidenceId {
    /// Creates an evidence identifier.
    ///
    /// # Errors
    ///
    /// Returns `ValueError` when the value violates the stable identifier contract.
    pub fn new(value: impl Into<String>) -> Result<Self, ValueError> {
        validate(value.into()).map(Self)
    }

    /// Returns the text of the evidence identifier.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for EvidenceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

fn validate(value: String) -> Result<String, ValueError> {
    let length = value.len();
    if value.is_empty() {
        return Err(ValueError::new(ValueViolation::Empty, length));
    }
    if length > MAX_ID_BYTES {
        return Err(ValueError::new(ValueViolation::TooLong, length));
    }
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return Err(ValueError::new(ValueViolation::Empty, length));
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(ValueError::new(ValueViolation::InvalidStart, length));
    }
    if !bytes.all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
    }) {
        return Err(ValueError::new(ValueViolation::InvalidCharacter, length));
    }
    Ok(value)
}

/// Exact SHA-256 digest used to bind builds and evidence.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvidenceDigest([u8; 32]);

impl EvidenceDigest {
    /// Wraps an already-computed SHA-256 digest.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for EvidenceDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EvidenceDigest(")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        formatter.write_str(")")
    }
}

impl fmt::Display for EvidenceDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}
