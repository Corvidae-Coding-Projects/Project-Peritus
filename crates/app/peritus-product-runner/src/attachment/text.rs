//! Exact bounded UTF-8 attachment bytes; validation never rewrites or truncates the source.

use crate::control::ControlError;
use peritus_types::Sha256Digest;

/// Maximum selected text bytes per explicit file reference.
pub const MAX_FILE_BYTES: u64 = 256 * 1024;
/// Maximum aggregate selected file text bytes in a request, excluding its other context.
pub const MAX_FILE_SELECTION_BYTES: u64 = 512 * 1024;
/// Maximum eligible file references in one request.
pub const MAX_FILE_COUNT: usize = 32;

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
    /// Rejects oversized data, invalid UTF-8 (including split codepoints), and binary/terminal
    /// control characters other than tab and line terminators. No lossy conversion occurs.
    pub fn new(bytes: Vec<u8>) -> Result<Self, ControlError> {
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(ControlError::Capacity);
        }
        let text = String::from_utf8(bytes).map_err(|_| ControlError::InvalidInput)?;
        if text.chars().any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')) {
            return Err(ControlError::InvalidInput);
        }
        let digest = peritus_codec::sha256(text.as_bytes());
        Ok(Self { text, digest })
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
