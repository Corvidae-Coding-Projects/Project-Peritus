//! Exact bounded UTF-8 attachment bytes; validation never rewrites or truncates the source.

use crate::control::ControlError;
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

/// Exact validated text, without filesystem authority or a claim of user consent.
#[derive(Clone, Eq, PartialEq)]
pub struct ValidatedFileText {
    text: String,
    digest: Sha256Digest,
}
impl ValidatedFileText {
    /// Validates original UTF-8 bytes, including empty files and original CRLF terminators.
    ///
    /// # Errors
    /// Rejects invalid UTF-8 (including split codepoints) and binary/terminal
    /// control characters other than tab and line terminators. No lossy conversion occurs.
    pub fn new(bytes: Vec<u8>) -> Result<Self, ControlError> {
        let digest = Self::validate_bytes(&bytes)?;
        let text = String::from_utf8(bytes).map_err(|_| ControlError::InvalidInput)?;
        Ok(Self { text, digest })
    }
    /// Validates exact UTF-8 bytes without retaining or reconstructing the source.
    ///
    /// # Errors
    /// Rejects invalid UTF-8 or binary/terminal controls.
    pub fn validate_bytes(bytes: &[u8]) -> Result<Sha256Digest, ControlError> {
        let text = std::str::from_utf8(bytes).map_err(|_| ControlError::InvalidInput)?;
        if text.chars().any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')) {
            return Err(ControlError::InvalidInput);
        }
        let mut digest = Sha256::new();
        for chunk in bytes.chunks(64 * 1024) {
            digest.update(chunk);
        }
        Ok(Sha256Digest::new(digest.finalize().into()))
    }
    /// Borrows exactly the validated source text; renderers still own display sanitization.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Returns SHA-256 of the unchanged selected bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Returns exact selected byte length.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.text.len() as u64
    }
}
impl std::fmt::Debug for ValidatedFileText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidatedFileText").field("bytes", &self.bytes()).finish_non_exhaustive()
    }
}
