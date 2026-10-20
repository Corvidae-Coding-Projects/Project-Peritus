//! Compatibility attachment API delegating validation to the client-safe shared model.
use crate::control::ControlError;
use peritus_types::Sha256Digest;

/// Exact validated text, without filesystem authority or a claim of user consent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedFileText(peritus_product_state::file_text::ValidatedFileText);
impl ValidatedFileText {
    /// Validates original UTF-8 bytes, preserving line endings and empty files.
    ///
    /// # Errors
    /// Rejects invalid UTF-8 and binary/terminal controls.
    pub fn new(bytes: Vec<u8>) -> Result<Self, ControlError> {
        peritus_product_state::file_text::ValidatedFileText::new(bytes)
            .map(Self)
            .map_err(|_| ControlError::InvalidInput)
    }
    /// Validates exact bytes without retaining a second source copy.
    ///
    /// # Errors
    /// Rejects invalid UTF-8 and binary/terminal controls.
    pub fn validate_bytes(bytes: &[u8]) -> Result<Sha256Digest, ControlError> {
        peritus_product_state::file_text::ValidatedFileText::validate_bytes(bytes)
            .map_err(|_| ControlError::InvalidInput)
    }
    /// Borrows exactly the validated source text; renderers still sanitize display.
    #[must_use]
    pub fn text(&self) -> &str {
        self.0.text()
    }
    /// Returns SHA-256 of the unchanged selected bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.0.digest()
    }
    /// Returns exact selected byte length.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.0.bytes()
    }
}
