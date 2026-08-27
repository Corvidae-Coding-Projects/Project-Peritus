//! Bounded participant and fresh-subject identities.

use std::fmt;

use serde::Serialize;

use crate::{QualificationError, QualificationErrorCode};

const MAX_ID_BYTES: usize = 128;

/// Identity of a release contributor, builder, signer, or auditor.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ParticipantId(String);

impl ParticipantId {
    /// Validates a lowercase portable participant identity.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] for empty, oversized, or nonportable input.
    pub fn new(value: impl Into<String>) -> Result<Self, QualificationError> {
        validate_identity(value.into()).map(Self)
    }

    /// Borrows the validated identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ParticipantId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Unique identity of one disposable fresh qualification subject.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SubjectId(String);

impl SubjectId {
    /// Validates a lowercase portable subject identity.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] for empty, oversized, or nonportable input.
    pub fn new(value: impl Into<String>) -> Result<Self, QualificationError> {
        validate_identity(value.into()).map(Self)
    }

    /// Borrows the validated identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SubjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

fn validate_identity(value: String) -> Result<String, QualificationError> {
    let mut bytes = value.bytes();
    let valid = value.len() <= MAX_ID_BYTES
        && bytes.next().is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && bytes.all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'-' | b'_' | b':' | b'/' | b'@')
        });
    if !valid {
        return Err(QualificationError::new(
            QualificationErrorCode::InvalidValue,
            "validate H4 identity",
            "identity violates the 1 through 128 byte lowercase portable grammar",
        ));
    }
    Ok(value)
}
