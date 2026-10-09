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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PreviewOutputMatchSource, PreviewOutputStream};

    fn matched(stream: PreviewOutputStream, digest: u8) -> PreviewOutputMatch {
        PreviewOutputMatch {
            stream,
            start_byte: 4,
            end_byte: 8,
            observed_stream_bytes: 16,
            matched_bytes_digest: [digest; 32],
            source: PreviewOutputMatchSource::LiveSpool {
                process_id: [7; 16],
                observed_prefix_digest: [digest.wrapping_add(1); 32],
            },
        }
    }

    #[test]
    fn evidence_allows_observed_stdout_and_note_stderr_from_same_process() {
        let evidence = GraphicalOutputEvidence::new(
            OperationId::new([1; 16]).expect("check id"),
            matched(PreviewOutputStream::Stdout, 2),
            matched(PreviewOutputStream::Stderr, 9),
        );

        assert!(evidence.is_ok());
    }

    #[test]
    fn evidence_rejects_matches_from_different_processes_or_source_modes() {
        let observed = matched(PreviewOutputStream::Stdout, 2);
        let mut other_process = matched(PreviewOutputStream::Stderr, 9);
        other_process.source = PreviewOutputMatchSource::LiveSpool {
            process_id: [8; 16],
            observed_prefix_digest: [10; 32],
        };
        assert!(
            GraphicalOutputEvidence::new(
                OperationId::new([1; 16]).expect("check id"),
                observed,
                other_process,
            )
            .is_err()
        );

        let terminal = matched(PreviewOutputStream::Terminal, 9);
        assert!(
            GraphicalOutputEvidence::new(
                OperationId::new([1; 16]).expect("check id"),
                observed,
                terminal,
            )
            .is_err()
        );
    }

    #[test]
    fn evidence_allows_live_and_finalized_snapshots_of_pipe_streams() {
        let observed = matched(PreviewOutputStream::Stdout, 2);
        let note = PreviewOutputMatch {
            source: PreviewOutputMatchSource::FinalizedArtifact {
                process_id: [7; 16],
                artifact_digest: [10; 32],
            },
            ..matched(PreviewOutputStream::Stderr, 9)
        };

        assert!(
            GraphicalOutputEvidence::new(
                OperationId::new([1; 16]).expect("check id"),
                observed,
                note,
            )
            .is_ok()
        );
    }
}
