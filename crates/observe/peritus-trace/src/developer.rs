//! Read-only decoding of the stable, length-framed product developer trace.

use std::{fmt, io, path::Path};
mod reader;
pub use reader::TraceReader;

/// Stable record kinds in the product runner's length-framed developer trace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeveloperTraceFrameKind {
    /// One canonical provider event envelope.
    ProviderEnvelope,
    /// One legacy unscoped tool observation.
    ToolObservation,
    /// One context-compaction record.
    ContextCompaction,
    /// One scheduled provider retry.
    RetryScheduled,
    /// One provider failover transition.
    ProviderSwitch,
    /// One role-scoped local-memory checkpoint.
    LocalMemoryCheckpoint,
    /// One role-scoped tool observation retained for local-memory recovery.
    LocalMemoryObservation,
}

impl DeveloperTraceFrameKind {
    /// Decodes a stable trace tag.
    #[must_use]
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::ProviderEnvelope),
            2 => Some(Self::ToolObservation),
            3 => Some(Self::ContextCompaction),
            4 => Some(Self::RetryScheduled),
            5 => Some(Self::ProviderSwitch),
            6 => Some(Self::LocalMemoryCheckpoint),
            7 => Some(Self::LocalMemoryObservation),
            _ => None,
        }
    }

    /// Returns the stable trace tag.
    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            Self::ProviderEnvelope => 1,
            Self::ToolObservation => 2,
            Self::ContextCompaction => 3,
            Self::RetryScheduled => 4,
            Self::ProviderSwitch => 5,
            Self::LocalMemoryCheckpoint => 6,
            Self::LocalMemoryObservation => 7,
        }
    }
}

/// One decoded record of a durable developer trace, in append order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceFrame {
    kind: DeveloperTraceFrameKind,
    payload: Vec<u8>,
}

impl TraceFrame {
    /// Stable record kind of this frame.
    #[must_use]
    pub const fn kind(&self) -> DeveloperTraceFrameKind {
        self.kind
    }

    /// Exact recorded payload bytes, unmodified.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

/// Why one durable developer trace could not be read.
#[derive(Debug)]
pub enum TraceReadError {
    /// The trace file could not be read from disk.
    Read(io::Error),
    /// The trace framing is corrupt: an unknown record tag, or a length that cannot be satisfied.
    Framing(String),
}

impl fmt::Display for TraceReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(formatter, "{error}"),
            Self::Framing(detail) => formatter.write_str(detail),
        }
    }
}

impl std::error::Error for TraceReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read(error) => Some(error),
            Self::Framing(_) => None,
        }
    }
}

/// Reads every complete record of one durable developer trace without modifying it.
///
/// A trailing incomplete record is ignored: the writer appends one synchronized frame at a time,
/// so an unterminated tail can only be a record a running daemon has not finished writing. An
/// unknown record tag is reported instead of silently discarded.
///
/// # Errors
///
/// Returns [`TraceReadError::Read`] when the trace cannot be read from disk and
/// [`TraceReadError::Framing`] when a record tag or declared length is not usable.
///
/// This convenience API collects all payloads. Use [`TraceReader`] to process arbitrarily long
/// traces with memory proportional to one selected record instead of the entire file.
pub fn read_frames(path: &Path) -> Result<Vec<TraceFrame>, TraceReadError> {
    let mut reader = TraceReader::open(path)?;
    let mut frames = Vec::new();
    while let Some(frame) = reader.next_matching(|_| true)? {
        frames.push(frame);
    }
    Ok(frames)
}
