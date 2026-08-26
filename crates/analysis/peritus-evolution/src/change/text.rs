//! Canonical bounded human-authored text.

use crate::{
    EvolutionError, EvolutionErrorKind, EvolutionLimits, EvolutionOperation, EvolutionRecovery,
};

/// Nonempty bounded UTF-8 text without surrounding whitespace or control characters.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BoundedText(String);

impl BoundedText {
    /// Validates one human-authored text field.
    ///
    /// # Errors
    /// Rejects empty, over-limit, non-trimmed, or control-containing text.
    pub fn new(value: String, limits: EvolutionLimits) -> Result<Self, EvolutionError> {
        if value.is_empty()
            || value.len() > usize::try_from(limits.text_bytes()).unwrap_or(usize::MAX)
            || value.trim() != value
            || value.chars().any(char::is_control)
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::InvalidInput,
                EvolutionOperation::AdmitManifest,
                EvolutionRecovery::CorrectInput,
                "manifest text is empty, noncanonical, or over limit",
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
