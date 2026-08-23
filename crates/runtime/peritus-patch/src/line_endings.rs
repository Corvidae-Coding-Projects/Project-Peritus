//! Explicit final-content line-ending policies.

use crate::{ErrorCode, PatchError, PatchOperationContext, RecoveryClass, RollbackStatus};

/// Transformation applied before final content identity is computed.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LineEndingPolicy {
    /// Preserve supplied bytes exactly, including binary content and mixed endings.
    Preserve,
    /// Validate UTF-8 text and normalize CRLF and lone CR terminators to LF.
    Lf,
    /// Validate UTF-8 text and normalize every terminator to CRLF.
    Crlf,
}

impl LineEndingPolicy {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::Preserve => 1,
            Self::Lf => 2,
            Self::Crlf => 3,
        }
    }

    pub(crate) fn transform(self, bytes: Vec<u8>) -> Result<Vec<u8>, PatchError> {
        if self == Self::Preserve {
            return Ok(bytes);
        }
        if bytes.contains(&0) || std::str::from_utf8(&bytes).is_err() {
            return Err(PatchError::message(
                ErrorCode::InvalidContent,
                RecoveryClass::CorrectPatch,
                PatchOperationContext::Plan,
                RollbackStatus::NotRequired,
                "line-ending normalization requires non-NUL UTF-8 text",
            ));
        }
        let mut lf = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                    lf.push(b'\n');
                    index += 2;
                }
                b'\r' => {
                    lf.push(b'\n');
                    index += 1;
                }
                byte => {
                    lf.push(byte);
                    index += 1;
                }
            }
        }
        if self == Self::Lf {
            return Ok(lf);
        }
        #[allow(
            clippy::naive_bytecount,
            reason = "the crate avoids a dependency for one bounded line-ending transform"
        )]
        let extra = lf.iter().filter(|byte| **byte == b'\n').count();
        let capacity = lf.len().checked_add(extra).ok_or_else(|| {
            PatchError::message(
                ErrorCode::ArithmeticOverflow,
                RecoveryClass::CorrectPatch,
                PatchOperationContext::Plan,
                RollbackStatus::NotRequired,
                "CRLF transformation length overflowed",
            )
        })?;
        let mut crlf = Vec::with_capacity(capacity);
        for byte in lf {
            if byte == b'\n' {
                crlf.push(b'\r');
            }
            crlf.push(byte);
        }
        Ok(crlf)
    }
}
