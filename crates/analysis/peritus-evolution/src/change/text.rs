//! Canonical bounded human-authored text.

use crate::{
    EvolutionError, EvolutionErrorKind, EvolutionLimits, EvolutionOperation, EvolutionRecovery,
};

/// Nonempty bounded UTF-8 text without surrounding whitespace or unsafe control characters.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BoundedText(String);

impl BoundedText {
    /// Validates one human-authored text field.
    ///
    /// # Errors
    /// Rejects empty, over-limit, non-trimmed, or unsafe-control-containing text.
    pub fn new(value: String, limits: EvolutionLimits) -> Result<Self, EvolutionError> {
        if value.is_empty()
            || limits
                .text_bytes_limit()
                .is_some_and(|maximum| value.len() > usize::try_from(maximum).unwrap_or(usize::MAX))
            || value.trim() != value
            || value.chars().any(|character| {
                character.is_control() && !matches!(character, '\n' | '\r' | '\t')
            })
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::InvalidInput,
                EvolutionOperation::AdmitManifest,
                EvolutionRecovery::CorrectInput,
                "manifest text is empty, noncanonical, contains unsafe controls, or is over limit",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the validated UTF-8 text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
