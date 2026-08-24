//! Typed process-stream offset observations.

/// Output stream owning one independently based offset observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessOutputStream {
    /// Standard output in pipe mode.
    Stdout,
    /// Standard error in pipe mode.
    Stderr,
    /// Combined terminal output in PTY mode.
    Terminal,
}

/// One typed stream offset retained in process-event observation order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessStreamOffsetObservation {
    stream: ProcessOutputStream,
    offset: u64,
}

impl ProcessStreamOffsetObservation {
    /// Creates one typed stream-offset observation.
    #[must_use]
    pub const fn new(stream: ProcessOutputStream, offset: u64) -> Self {
        Self { stream, offset }
    }

    /// Returns the independently based stream.
    #[must_use]
    pub const fn stream(self) -> ProcessOutputStream {
        self.stream
    }

    /// Returns the byte offset within that stream.
    #[must_use]
    pub const fn offset(self) -> u64 {
        self.offset
    }
}
