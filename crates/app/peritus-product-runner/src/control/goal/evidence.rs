//! Exact output-search evidence bound to one graphical goal check operation.

use crate::{PreviewOutputMatch, PreviewOutputStream};
use serde::{Deserialize, Serialize};

use super::{ControlError, OperationId};

/// Durable binding between a graphical behavior check and its observed output matches.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GraphicalOutputEvidence {
    check: OperationId,
    observed: PreviewOutputMatch,
    note: PreviewOutputMatch,
}

impl GraphicalOutputEvidence {
    /// Creates evidence after checking process identity and terminal-versus-pipes mode.
    ///
    /// # Errors
    /// Rejects invalid operation ids, malformed ranges, or inconsistent process/output mode.
    pub fn new(
        check: OperationId,
        observed: PreviewOutputMatch,
        note: PreviewOutputMatch,
    ) -> Result<Self, ControlError> {
        let value = Self { check, observed, note };
        value.validate()?;
        Ok(value)
    }

    /// Exact original `CheckPreviewBehavior` operation id.
    #[must_use]
    pub const fn check(self) -> OperationId {
        self.check
    }

    /// Exact byte evidence for the behavior observation.
    #[must_use]
    pub const fn observed(self) -> PreviewOutputMatch {
        self.observed
    }

    /// Exact byte evidence for the graphical criterion note.
    #[must_use]
    pub const fn note(self) -> PreviewOutputMatch {
        self.note
    }

    /// Validates all evidence after deserialization.
    ///
    /// # Errors
    /// Rejects malformed ranges or evidence from different processes or terminal/pipe modes.
    pub fn validate(self) -> Result<(), ControlError> {
        self.observed.validate().map_err(|_| ControlError::InvalidInput)?;
        self.note.validate().map_err(|_| ControlError::InvalidInput)?;
        let same_process = self.observed.process_id_bytes() == self.note.process_id_bytes();
        let same_mode = (self.observed.stream() == PreviewOutputStream::Terminal)
            == (self.note.stream() == PreviewOutputStream::Terminal);
        if !same_process || !same_mode {
            return Err(ControlError::InvalidInput);
        }
        Ok(())
    }
}
