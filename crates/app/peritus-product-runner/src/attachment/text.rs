//! Exact UTF-8 attachment bytes; validation never rewrites or truncates the source.

use crate::control::ControlError;
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};
use std::io::{Read, Seek, SeekFrom};

/// Legacy compatibility value; production text selection has no host byte ceiling.
pub const MAX_FILE_BYTES: u64 = 256 * 1024;

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
    /// Rejects invalid UTF-8 and binary/terminal control characters other than tab and line
    /// terminators. No lossy conversion occurs.
    pub fn new(bytes: Vec<u8>) -> Result<Self, ControlError> {
        let text = String::from_utf8(bytes).map_err(|_| ControlError::InvalidInput)?;
        validate_text(&text)?;
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
    /// Returns the exact validated bytes without another body-sized allocation.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.text.into_bytes()
    }
}
impl std::fmt::Debug for ValidatedFileText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidatedFileText").field("bytes", &self.bytes()).finish_non_exhaustive()
    }
}

/// Fixed-memory validation evidence over caller-owned seekable selected content.
///
/// Construction scans the complete selection, verifies its immutable digest and length, and
/// rewinds it. The reader remains owner-controlled and can then be streamed into durable storage.
pub struct ValidatedFileTextSource<R> {
    reader: R,
    digest: Sha256Digest,
    bytes: u64,
}

impl<R: Read + Seek> ValidatedFileTextSource<R> {
    /// Validates complete selected content against its observed immutable identity.
    ///
    /// # Errors
    /// Rejects read/seek failures, length or digest drift, invalid UTF-8 including split or
    /// incomplete code points, and disallowed control characters. No total-size ceiling applies.
    pub fn new(
        mut reader: R,
        expected_digest: Sha256Digest,
        expected_bytes: u64,
    ) -> Result<Self, ControlError> {
        reader.seek(SeekFrom::Start(0)).map_err(|_| ControlError::InvalidInput)?;
        let mut hash = Sha256::new();
        let mut total = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        let mut pending = Vec::with_capacity(buffer.len() + 3);
        loop {
            let count = reader.read(&mut buffer).map_err(|_| ControlError::InvalidInput)?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(u64::try_from(count).map_err(|_| ControlError::Capacity)?)
                .ok_or(ControlError::Capacity)?;
            if total > expected_bytes {
                return Err(ControlError::InvalidInput);
            }
            hash.update(&buffer[..count]);
            validate_chunk(&mut pending, &buffer[..count])?;
        }
        if total != expected_bytes || !pending.is_empty() {
            return Err(ControlError::InvalidInput);
        }
        let digest = Sha256Digest::new(hash.finalize().into());
        if digest != expected_digest {
            return Err(ControlError::InvalidInput);
        }
        reader.seek(SeekFrom::Start(0)).map_err(|_| ControlError::InvalidInput)?;
        Ok(Self { reader, digest, bytes: total })
    }

    /// Borrows the validated reader for one exact streamed publication.
    pub fn reader_mut(&mut self) -> &mut R {
        &mut self.reader
    }

    /// Returns SHA-256 of the unchanged selected bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Returns exact selected byte length.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }
}

impl<R> std::fmt::Debug for ValidatedFileTextSource<R> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ValidatedFileTextSource")
            .field("bytes", &self.bytes)
            .field("digest", &self.digest)
            .finish_non_exhaustive()
    }
}

fn validate_text(text: &str) -> Result<(), ControlError> {
    if text.chars().any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')) {
        return Err(ControlError::InvalidInput);
    }
    Ok(())
}

fn validate_chunk(pending: &mut Vec<u8>, bytes: &[u8]) -> Result<(), ControlError> {
    pending.extend_from_slice(bytes);
    match std::str::from_utf8(pending) {
        Ok(text) => {
            validate_text(text)?;
            pending.clear();
        }
        Err(error) => {
            if error.error_len().is_some() {
                return Err(ControlError::InvalidInput);
            }
            let valid = error.valid_up_to();
            validate_text(
                std::str::from_utf8(&pending[..valid])
                    .map_err(|_| ControlError::InvalidInput)?,
            )?;
            pending.drain(..valid);
            if pending.len() > 3 {
                return Err(ControlError::InvalidInput);
            }
        }
    }
    Ok(())
}
