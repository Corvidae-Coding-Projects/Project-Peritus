//! Pure terminal attachment ordering state machine.

use super::{
    TerminalBinding, TerminalCancellation, TerminalDetach, TerminalError, TerminalErrorKind,
    TerminalExit, TerminalInput, TerminalOutput, TerminalOutputGap, TerminalResize, TerminalStream,
    error::reject,
    output_is_contiguous,
};

/// Observable terminal attachment phase.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TerminalPhase {
    /// Input, resize, output, detach, cancel, and exit observations are admissible.
    Attached,
    /// A detach fact ended this attachment; repeating it exactly is idempotent.
    Detached(TerminalDetach),
    /// A cancellation fact ended this attachment.
    Cancelled(TerminalCancellation),
    /// One exact exit observation ended this attachment.
    Exited(TerminalExit),
}

/// Result of idempotently applying a detach or cancellation fact.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TerminalTransitionDisposition {
    /// The fact caused the terminal transition.
    Applied,
    /// The exact retained fact was repeated.
    Repeated,
}

/// Pure attachment state with globally contiguous output ordering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalState {
    binding: TerminalBinding,
    maximum_chunk_bytes: usize,
    next_output_sequence: u64,
    next_output_offset: u64,
    stream_offsets: [u64; 3],
    phase: TerminalPhase,
}

impl TerminalState {
    /// Creates an attached terminal with zero-based output sequence and offset.
    ///
    /// # Errors
    ///
    /// Rejects a zero negotiated terminal chunk bound.
    pub const fn new(
        binding: TerminalBinding,
        maximum_chunk_bytes: usize,
    ) -> Result<Self, TerminalError> {
        if maximum_chunk_bytes == 0 {
            return Err(reject(TerminalErrorKind::InvalidLimit, "terminal chunk limit is zero"));
        }
        Ok(Self {
            binding,
            maximum_chunk_bytes,
            next_output_sequence: 0,
            next_output_offset: 0,
            stream_offsets: [0; 3],
            phase: TerminalPhase::Attached,
        })
    }

    /// Returns the immutable attachment/process/action binding.
    #[must_use]
    pub const fn binding(&self) -> TerminalBinding {
        self.binding
    }
    /// Returns the exact next output sequence.
    #[must_use]
    pub const fn next_output_sequence(&self) -> u64 {
        self.next_output_sequence
    }
    /// Returns the exact next global byte offset.
    #[must_use]
    pub const fn next_output_offset(&self) -> u64 {
        self.next_output_offset
    }
    /// Returns stdout, stderr, and combined-terminal byte frontiers in stable order.
    #[must_use]
    pub const fn stream_offsets(&self) -> [u64; 3] {
        self.stream_offsets
    }
    /// Returns the observable phase.
    #[must_use]
    pub const fn phase(&self) -> TerminalPhase {
        self.phase
    }

    /// Admits one exact attachment-bound output chunk and advances both output fences.
    ///
    /// # Errors
    ///
    /// Rejects terminal state, binding mismatch, bound violation, wrong sequence/offset, or
    /// arithmetic overflow.
    pub fn accept_output(&mut self, output: &TerminalOutput) -> Result<(), TerminalError> {
        self.require_attached(output.binding())?;
        if output.bytes().len() > self.maximum_chunk_bytes {
            return Err(reject(
                TerminalErrorKind::InvalidInput,
                "output exceeds this attachment's negotiated chunk bound",
            ));
        }
        if output.sequence() != self.next_output_sequence {
            return Err(reject(
                TerminalErrorKind::UnexpectedSequence,
                "output sequence is not the exact expected sequence",
            ));
        }
        if output.offset() != self.next_output_offset {
            return Err(reject(
                TerminalErrorKind::UnexpectedOffset,
                "output offset is not the exact conserved offset",
            ));
        }
        if !output_is_contiguous(
            self.next_output_sequence,
            self.next_output_offset,
            output.sequence(),
            output.offset(),
            output.bytes().len(),
        ) {
            return Err(reject(
                TerminalErrorKind::ArithmeticOverflow,
                "output position arithmetic overflow",
            ));
        }
        let length = u64::try_from(output.bytes().len()).map_err(|_| {
            reject(TerminalErrorKind::ArithmeticOverflow, "output length does not fit u64")
        })?;
        let next_output_offset = self.next_output_offset.checked_add(length).ok_or_else(|| {
            reject(TerminalErrorKind::ArithmeticOverflow, "output offset overflow")
        })?;
        let next_output_sequence = self.next_output_sequence.checked_add(1).ok_or_else(|| {
            reject(TerminalErrorKind::ArithmeticOverflow, "output sequence overflow")
        })?;
        let stream = stream_index(output.stream());
        let next_stream_offset = self.stream_offsets[stream].checked_add(length).ok_or_else(|| {
            reject(TerminalErrorKind::ArithmeticOverflow, "stream offset overflow")
        })?;
        self.next_output_offset = next_output_offset;
        self.next_output_sequence = next_output_sequence;
        self.stream_offsets[stream] = next_stream_offset;
        Ok(())
    }

    /// Admits one explicit unavailable range and advances to its exact retained frontier.
    ///
    /// # Errors
    ///
    /// Rejects terminal state, binding/sequence/offset mismatch, regressing stream frontiers,
    /// nonconserved global and per-stream byte deltas, or arithmetic overflow.
    pub fn accept_output_gap(&mut self, gap: TerminalOutputGap) -> Result<(), TerminalError> {
        self.require_attached(gap.binding())?;
        if gap.sequence() != self.next_output_sequence {
            return Err(reject(
                TerminalErrorKind::UnexpectedSequence,
                "output gap sequence is not the exact expected sequence",
            ));
        }
        if gap.offset() != self.next_output_offset {
            return Err(reject(
                TerminalErrorKind::UnexpectedOffset,
                "output gap does not start at the exact conserved offset",
            ));
        }
        let resumed = gap.stream_offsets();
        let mut missing = 0_u64;
        for (before, after) in self.stream_offsets.into_iter().zip(resumed) {
            let delta = after.checked_sub(before).ok_or_else(|| {
                reject(TerminalErrorKind::UnexpectedOffset, "output gap regresses a stream offset")
            })?;
            missing = missing.checked_add(delta).ok_or_else(|| {
                reject(TerminalErrorKind::ArithmeticOverflow, "output gap length overflow")
            })?;
        }
        if missing != gap.missing_bytes() {
            return Err(reject(
                TerminalErrorKind::UnexpectedOffset,
                "output gap stream ranges do not conserve its global byte range",
            ));
        }
        let next_output_sequence = self.next_output_sequence.checked_add(1).ok_or_else(|| {
            reject(TerminalErrorKind::ArithmeticOverflow, "output sequence overflow")
        })?;
        self.next_output_sequence = next_output_sequence;
        self.next_output_offset = gap.resume_offset();
        self.stream_offsets = resumed;
        Ok(())
    }

    /// Validates one terminal input request without performing process I/O.
    ///
    /// # Errors
    ///
    /// Rejects terminal state, binding mismatch, or a value above this attachment's byte bound.
    pub fn accept_input(&self, input: &TerminalInput) -> Result<(), TerminalError> {
        self.require_attached(input.binding())?;
        if input.bytes().len() > self.maximum_chunk_bytes {
            return Err(reject(
                TerminalErrorKind::InvalidInput,
                "input exceeds this attachment's negotiated chunk bound",
            ));
        }
        Ok(())
    }

    /// Validates one positive bounded resize without performing process I/O.
    ///
    /// # Errors
    ///
    /// Rejects terminal state or binding mismatch.
    pub fn resize(&self, resize: TerminalResize) -> Result<(), TerminalError> {
        self.require_attached(resize.binding())
    }

    /// Applies a matching detach fact idempotently.
    ///
    /// # Errors
    ///
    /// Rejects binding mismatch or a conflicting terminal fact.
    pub fn detach(
        &mut self,
        detach: TerminalDetach,
    ) -> Result<TerminalTransitionDisposition, TerminalError> {
        self.require_binding(detach.binding())?;
        match self.phase {
            TerminalPhase::Attached => {
                self.phase = TerminalPhase::Detached(detach);
                Ok(TerminalTransitionDisposition::Applied)
            }
            TerminalPhase::Detached(retained) if retained == detach => {
                Ok(TerminalTransitionDisposition::Repeated)
            }
            _ => Err(reject(
                TerminalErrorKind::TerminalConflict,
                "detach conflicts with the retained terminal fact",
            )),
        }
    }

    /// Applies a matching cancellation fact idempotently.
    ///
    /// # Errors
    ///
    /// Rejects binding mismatch or a conflicting terminal fact.
    pub fn cancel(
        &mut self,
        cancellation: TerminalCancellation,
    ) -> Result<TerminalTransitionDisposition, TerminalError> {
        self.require_binding(cancellation.binding())?;
        match self.phase {
            TerminalPhase::Attached => {
                self.phase = TerminalPhase::Cancelled(cancellation);
                Ok(TerminalTransitionDisposition::Applied)
            }
            TerminalPhase::Cancelled(retained) if retained == cancellation => {
                Ok(TerminalTransitionDisposition::Repeated)
            }
            _ => Err(reject(
                TerminalErrorKind::TerminalConflict,
                "cancellation conflicts with the retained terminal fact",
            )),
        }
    }

    /// Accepts exactly one matching exit observation at the exact output fence.
    ///
    /// # Errors
    ///
    /// Rejects terminal state, binding mismatch, a second exit, or a stale/future output fence.
    pub fn exit(&mut self, exit: TerminalExit) -> Result<(), TerminalError> {
        self.require_attached(exit.binding())?;
        if matches!(exit.disposition(), super::TerminalExitDisposition::Signal(0)) {
            return Err(reject(
                TerminalErrorKind::InvalidInput,
                "numeric terminal exit signal must be positive",
            ));
        }
        if exit.next_sequence() != self.next_output_sequence {
            return Err(reject(
                TerminalErrorKind::UnexpectedSequence,
                "exit does not fence the exact next output sequence",
            ));
        }
        if exit.final_offset() != self.next_output_offset {
            return Err(reject(
                TerminalErrorKind::UnexpectedOffset,
                "exit does not fence the exact final output offset",
            ));
        }
        self.phase = TerminalPhase::Exited(exit);
        Ok(())
    }

    fn require_attached(&self, binding: TerminalBinding) -> Result<(), TerminalError> {
        self.require_binding(binding)?;
        if self.phase != TerminalPhase::Attached {
            return Err(reject(
                TerminalErrorKind::AlreadyTerminal,
                "terminal attachment is already terminal",
            ));
        }
        Ok(())
    }

    fn require_binding(&self, binding: TerminalBinding) -> Result<(), TerminalError> {
        if binding != self.binding {
            return Err(reject(
                TerminalErrorKind::BindingMismatch,
                "operation names another terminal attachment",
            ));
        }
        Ok(())
    }
}

const fn stream_index(stream: TerminalStream) -> usize {
    match stream {
        TerminalStream::Stdout => 0,
        TerminalStream::Stderr => 1,
        TerminalStream::Terminal => 2,
    }
}
