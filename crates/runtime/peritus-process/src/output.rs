//! Bounded stream accounting and retained output.

mod spool;
mod window;

pub(crate) use spool::{BoundedSpool, SpoolSet};
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
    /// Returns exact observed bytes not retained because of a ceiling or I/O failure.
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

impl OutputAccounting {
    pub(crate) const fn new(stream: OutputStream, ceiling: Option<u64>) -> Self {
        Self { stream, ceiling, observed: 0, retained: 0, dropped: 0, failed: false }
    }

    pub(crate) fn observe(&mut self, bytes: usize, external_available: u64) -> usize {
        let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
        self.observed = self.observed.saturating_add(bytes);
        let available = self
            .ceiling
            .map_or(u64::MAX, |ceiling| ceiling.saturating_sub(self.retained))
            .min(external_available);
        let accepted = available.min(bytes);
        // Observation alone does not prove durable retention. The spool writer commits the
        // successfully written prefix separately, including any prefix before an I/O error.
        self.dropped = self.dropped.saturating_add(bytes);
        usize::try_from(accepted).unwrap_or(usize::MAX)
    }

    pub(crate) const fn retain_written(&mut self, bytes: u64) {
        self.retained = self.retained.saturating_add(bytes);
        self.dropped = self.dropped.saturating_sub(bytes);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_spool_write_retains_only_the_successfully_written_prefix() {
        let mut accounting = OutputAccounting::new(OutputStream::Stdout, None);
        assert_eq!(accounting.observe(8, u64::MAX), 8);
        accounting.retain_written(3);
        accounting.fail();
        let result = accounting.finish();
        assert_eq!(result.observed(), 8);
        assert_eq!(result.retained(), 3);
        assert_eq!(result.dropped(), 5);
        assert_eq!(result.completeness(), OutputCompleteness::Incomplete);
    }

    #[test]
    fn absent_output_quota_retains_every_observed_byte() {
        let mut accounting = OutputAccounting::new(OutputStream::Stdout, None);
        assert_eq!(accounting.observe(32 * 1024 * 1024, u64::MAX), 32 * 1024 * 1024);
        accounting.retain_written(32 * 1024 * 1024);
        let result = accounting.finish();
        assert_eq!(result.observed(), 32 * 1024 * 1024);
        assert_eq!(result.retained(), result.observed());
        assert_eq!(result.dropped(), 0);
        assert_eq!(result.completeness(), OutputCompleteness::Complete);
    }
}
