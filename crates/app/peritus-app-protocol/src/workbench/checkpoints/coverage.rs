//! Explicit restore scope; full source identities never widen selected-range authority.

use super::{AppProtocolError, WorkbenchCheckpointVersion, invalid, valid_path};
use crate::WorkbenchFileRange;

/// One selected range and its half-open byte interval in the checkpoint snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchCheckpointRange {
    selection: WorkbenchFileRange,
    start: u64,
    end: u64,
}
impl WorkbenchCheckpointRange {
    /// Binds a nonempty byte or line selection to its captured interval.
    ///
    /// # Errors
    /// Rejects whole-file, empty, malformed, or inconsistent byte selections.
    pub const fn new(
        selection: WorkbenchFileRange,
        start: u64,
        end: u64,
    ) -> Result<Self, AppProtocolError> {
        let valid = match selection {
            WorkbenchFileRange::Bytes { start: first, end: last } => first == start && last == end,
            WorkbenchFileRange::Lines { first, last } => first > 0 && first <= last,
            WorkbenchFileRange::All => false,
        };
        if !valid || start >= end {
            return Err(invalid());
        }
        Ok(Self { selection, start, end })
    }
    /// Returns the source descriptor used to resolve the range against current bytes.
    #[must_use]
    pub const fn selection(self) -> WorkbenchFileRange {
        self.selection
    }
    /// Returns the interval in the retained capture snapshot.
    #[must_use]
    pub const fn captured_interval(self) -> (u64, u64) {
        (self.start, self.end)
    }
    fn canonical_key(self) -> (u64, u64, u8, u64, u64) {
        let (kind, first, last) = match self.selection {
            WorkbenchFileRange::All => (0, 0, 0),
            WorkbenchFileRange::Bytes { start, end } => (1, start, end),
            WorkbenchFileRange::Lines { first, last } => (2, u64::from(first), u64::from(last)),
        };
        (self.start, self.end, kind, first, last)
    }
}

/// Exact covered restoration capability shown before confirmation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchCheckpointCoverage<'a> {
    /// Complete regular-file content and portable mode.
    WholeFile,
    /// Only selected intervals; surrounding current bytes and mode are preserved.
    SelectedRanges(&'a [WorkbenchCheckpointRange]),
    /// Exact absence of a workspace target.
    AbsentPath,
    /// Empty-directory type and permissions.
    EmptyDirectory,
}

/// One exact covered target and its completed owned version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchCheckpointPath {
    path: String,
    checkpoint: WorkbenchCheckpointVersion,
    expected_current: Option<WorkbenchCheckpointVersion>,
    ranges: Vec<WorkbenchCheckpointRange>,
}
impl WorkbenchCheckpointPath {
    /// Validates one canonical relative target.
    ///
    /// # Errors
    /// Rejects malformed paths or directory permission encodings.
    pub fn new(
        path: String,
        checkpoint: WorkbenchCheckpointVersion,
        expected_current: Option<WorkbenchCheckpointVersion>,
    ) -> Result<Self, AppProtocolError> {
        if !valid_path(&path) {
            return Err(invalid());
        }
        checkpoint.validate()?;
        if let Some(expected) = expected_current {
            expected.validate()?;
        }
        Ok(Self { path, checkpoint, expected_current, ranges: Vec::new() })
    }
    /// Limits this target to canonical selected ranges without changing its integrity identity.
    ///
    /// # Errors
    /// Rejects absent/non-file capture sources, unordered or duplicate ranges, and intervals
    /// outside the complete capture snapshot. An empty list retains the whole-path capability.
    pub fn with_ranges(
        mut self,
        ranges: Vec<WorkbenchCheckpointRange>,
    ) -> Result<Self, AppProtocolError> {
        validate_ranges(self.checkpoint, &ranges)?;
        self.ranges = ranges;
        Ok(self)
    }
    /// Borrows canonical relative path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
    /// Returns the complete capture identity, not permission to restore surrounding bytes.
    #[must_use]
    pub const fn checkpoint(&self) -> WorkbenchCheckpointVersion {
        self.checkpoint
    }
    /// Returns completed owned post-change version when sealed.
    #[must_use]
    pub const fn expected_current(&self) -> Option<WorkbenchCheckpointVersion> {
        self.expected_current
    }
    /// Borrows canonical selected ranges; empty means the entire typed target is covered.
    #[must_use]
    pub fn ranges(&self) -> &[WorkbenchCheckpointRange] {
        &self.ranges
    }
    /// Returns the explicit restoration scope.
    #[must_use]
    pub fn coverage(&self) -> WorkbenchCheckpointCoverage<'_> {
        coverage(self.checkpoint, &self.ranges)
    }
}

pub(super) fn validate_ranges(
    version: WorkbenchCheckpointVersion,
    ranges: &[WorkbenchCheckpointRange],
) -> Result<(), AppProtocolError> {
    if ranges.is_empty() {
        return Ok(());
    }
    let WorkbenchCheckpointVersion::Present { bytes, .. } = version else {
        return Err(invalid());
    };
    if ranges.iter().any(|range| range.end > bytes)
        || ranges.windows(2).any(|pair| pair[0].canonical_key() >= pair[1].canonical_key())
    {
        return Err(invalid());
    }
    Ok(())
}

pub(super) const fn coverage(
    version: WorkbenchCheckpointVersion,
    ranges: &[WorkbenchCheckpointRange],
) -> WorkbenchCheckpointCoverage<'_> {
    if !ranges.is_empty() {
        return WorkbenchCheckpointCoverage::SelectedRanges(ranges);
    }
    match version {
        WorkbenchCheckpointVersion::Absent => WorkbenchCheckpointCoverage::AbsentPath,
        WorkbenchCheckpointVersion::Present { .. } => WorkbenchCheckpointCoverage::WholeFile,
        WorkbenchCheckpointVersion::EmptyDirectory { .. } => {
            WorkbenchCheckpointCoverage::EmptyDirectory
        }
    }
}
