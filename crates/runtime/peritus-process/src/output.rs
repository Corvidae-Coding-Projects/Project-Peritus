//! Bounded stream accounting and retained output.

mod spool;
mod window;

use crate::ProcessError;

pub(crate) use spool::{SegmentedSpool, SpoolSet, stream_spool_name};
pub(crate) use window::RetainedWindow;

/// Stable process output stream.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OutputStream {
    /// Pipe standard output.
    Stdout,
    /// Pipe standard error.
    Stderr,
    /// Combined PTY terminal data.
    Terminal,
}

/// Whether terminal output is exact and complete.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OutputCompleteness {
    /// Every observed byte was retained through EOF.
    Complete,
    /// A configured ceiling caused exact counted truncation.
    Truncated,
    /// I/O or recovery prevented a complete observation.
    Incomplete,
}

/// Exact terminal accounting for one stream.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StreamAccounting {
    stream: OutputStream,
    observed: u64,
    retained: u64,
    dropped: u64,
    completeness: OutputCompleteness,
}

impl StreamAccounting {
    pub(crate) const fn from_persisted(
        stream: OutputStream,
        observed: u64,
        retained: u64,
        dropped: u64,
        completeness: OutputCompleteness,
    ) -> Option<Self> {
        if retained > observed || dropped != observed - retained {
            return None;
        }
        Some(Self { stream, observed, retained, dropped, completeness })
    }

    /// Returns the stream.
    #[must_use]
    pub const fn stream(self) -> OutputStream {
        self.stream
    }
    /// Returns all bytes read from the OS stream.
    #[must_use]
    pub const fn observed(self) -> u64 {
        self.observed
    }
    /// Returns bytes retained in the durable spool.
    #[must_use]
    pub const fn retained(self) -> u64 {
        self.retained
    }
    /// Returns exact bytes not retained after a ceiling.
    #[must_use]
    pub const fn dropped(self) -> u64 {
        self.dropped
    }
    /// Returns output completeness.
    #[must_use]
    pub const fn completeness(self) -> OutputCompleteness {
        self.completeness
    }
}

pub(crate) struct OutputAccounting {
    stream: OutputStream,
    ceiling: Option<u64>,
    observed: u64,
    retained: u64,
    dropped: u64,
    failed: bool,
}

pub(crate) struct OutputReservation {
    observed: u64,
    bytes: u64,
    accepted: usize,
}

impl OutputReservation {
    pub(crate) const fn accepted(&self) -> usize {
        self.accepted
    }
}

impl OutputAccounting {
    pub(crate) const fn new(stream: OutputStream, ceiling: Option<u64>) -> Self {
        Self { stream, ceiling, observed: 0, retained: 0, dropped: 0, failed: false }
    }

    pub(crate) fn reserve(
        &self,
        bytes: usize,
        external_available: Option<u64>,
    ) -> Result<OutputReservation, ProcessError> {
        let bytes = u64::try_from(bytes)
            .map_err(|_| output_accounting_error("output chunk length is unrepresentable"))?;
        let observed = self
            .observed
            .checked_add(bytes)
            .ok_or_else(|| output_accounting_error("observed output accounting overflowed"))?;
        // A spool failure can leave only an exact known prefix durable. Keep observing later
        // bytes, but never append a suffix after the failed range: the retained artifact must
        // remain that exact prefix.
        let mut accepted = if self.failed { 0 } else { bytes };
        if let Some(ceiling) = self.ceiling {
            let available = ceiling
                .checked_sub(self.retained)
                .ok_or_else(|| output_accounting_error("retained output exceeds its allowance"))?;
            accepted = accepted.min(available);
        }
        if let Some(available) = external_available {
            accepted = accepted.min(available);
        }
        self
            .retained
            .checked_add(accepted)
            .ok_or_else(|| output_accounting_error("retained output accounting overflowed"))?;
        self
            .dropped
            .checked_add(bytes)
            .ok_or_else(|| output_accounting_error("dropped output accounting overflowed"))?;
        let accepted = usize::try_from(accepted)
            .map_err(|_| output_accounting_error("accepted output length is unrepresentable"))?;
        Ok(OutputReservation { observed, bytes, accepted })
    }

    pub(crate) fn commit(
        &mut self,
        reservation: OutputReservation,
        retained: u64,
    ) -> Result<(), ProcessError> {
        let accepted = u64::try_from(reservation.accepted)
            .map_err(|_| output_accounting_error("accepted output length is unrepresentable"))?;
        if retained > accepted {
            return Err(output_accounting_error(
                "persisted output exceeds its reserved prefix",
            ));
        }
        self.observed = reservation.observed;
        self.retained = self
            .retained
            .checked_add(retained)
            .ok_or_else(|| output_accounting_error("retained output accounting overflowed"))?;
        self.dropped = self
            .dropped
            .checked_add(reservation.bytes - retained)
            .ok_or_else(|| output_accounting_error("dropped output accounting overflowed"))?;
        Ok(())
    }

    pub(crate) const fn exceeded(&self) -> bool {
        self.dropped > 0
    }
    pub(crate) const fn observed(&self) -> u64 {
        self.observed
    }
    pub(crate) const fn fail(&mut self) {
        self.failed = true;
    }

    pub(crate) const fn finish(self) -> StreamAccounting {
        StreamAccounting {
            stream: self.stream,
            observed: self.observed,
            retained: self.retained,
            dropped: self.dropped,
            completeness: if self.failed {
                OutputCompleteness::Incomplete
            } else if self.dropped > 0 {
                OutputCompleteness::Truncated
            } else {
                OutputCompleteness::Complete
            },
        }
    }
}

const fn output_accounting_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        crate::ErrorCode::Output,
        crate::ProcessOperation::Stream,
        crate::RecoveryClass::CancelAndReap,
        detail,
    )
}
