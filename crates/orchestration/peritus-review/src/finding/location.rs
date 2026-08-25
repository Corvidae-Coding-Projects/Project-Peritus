//! Checked repository-relative source locations.

#![allow(
    clippy::redundant_pub_crate,
    reason = "the private location module exposes inert reconstruction to sibling wire code"
)]

use crate::ReviewLimits;
use crate::error::{ReviewError, ReviewErrorKind, reject};

/// Nonempty inclusive source coordinate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct SourceCoordinate {
    line: u32,
    column: u32,
}

impl SourceCoordinate {
    /// Creates a nonzero source coordinate.
    ///
    /// # Errors
    /// Rejects line zero or column zero.
    pub(crate) fn new(line: u32, column: u32) -> Result<Self, ReviewError> {
        if line == 0 || column == 0 {
            Err(reject(
                ReviewErrorKind::InvalidInput,
                "source line and column must both be nonzero",
            ))
        } else {
            Ok(Self { line, column })
        }
    }

    pub(crate) const fn from_wire(line: u32, column: u32) -> Self {
        Self { line, column }
    }
}

/// Bounded repository-relative UTF-8 path and inclusive source range.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FindingLocation {
    path: String,
    start: SourceCoordinate,
    end: SourceCoordinate,
}

impl FindingLocation {
    /// Creates a checked repository-relative source range.
    ///
    /// # Errors
    /// Rejects empty/absolute/traversing paths, oversized paths, zero coordinates, or reversed
    /// inclusive ranges.
    pub fn new(
        path: String,
        start_line: u32,
        start_column: u32,
        end_line: u32,
        end_column: u32,
        limits: ReviewLimits,
    ) -> Result<Self, ReviewError> {
        let start = SourceCoordinate::new(start_line, start_column)?;
        let end = SourceCoordinate::new(end_line, end_column)?;
        let location = Self { path, start, end };
        location.validate(limits)?;
        Ok(location)
    }

    pub(crate) const fn from_wire(
        path: String,
        start_line: u32,
        start_column: u32,
        end_line: u32,
        end_column: u32,
    ) -> Self {
        Self {
            path,
            start: SourceCoordinate::from_wire(start_line, start_column),
            end: SourceCoordinate::from_wire(end_line, end_column),
        }
    }

    /// Borrows the normalized repository-relative path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
    /// Returns the inclusive start coordinate.
    /// Returns the inclusive end coordinate.
    /// Returns the one-based inclusive start line.
    #[must_use]
    pub const fn start_line(&self) -> u32 {
        self.start.line
    }
    /// Returns the one-based inclusive start column.
    #[must_use]
    pub const fn start_column(&self) -> u32 {
        self.start.column
    }
    /// Returns the one-based inclusive end line.
    #[must_use]
    pub const fn end_line(&self) -> u32 {
        self.end.line
    }
    /// Returns the one-based inclusive end column.
    #[must_use]
    pub const fn end_column(&self) -> u32 {
        self.end.column
    }

    pub(crate) fn validate(&self, limits: ReviewLimits) -> Result<(), ReviewError> {
        if self.path.is_empty() || self.path.len() > limits.path_bytes() as usize {
            return Err(reject(
                ReviewErrorKind::LimitExceeded,
                "finding path is empty or exceeds its byte limit",
            ));
        }
        if self.path.starts_with('/')
            || self.path.starts_with('\\')
            || self.path.as_bytes().get(1).is_some_and(|value| *value == b':')
            || self
                .path
                .split(['/', '\\'])
                .any(|component| component.is_empty() || component == "." || component == "..")
        {
            return Err(reject(
                ReviewErrorKind::InvalidInput,
                "finding path is not a normalized repository-relative path",
            ));
        }
        if self.start.line == 0
            || self.start.column == 0
            || self.end.line == 0
            || self.end.column == 0
            || self.start > self.end
        {
            return Err(reject(
                ReviewErrorKind::InvalidInput,
                "finding source range is zero or reversed",
            ));
        }
        Ok(())
    }
}
