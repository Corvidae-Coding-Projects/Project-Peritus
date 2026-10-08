//! Typed checkpoint coverage, selected intervals and exact owned versions.

use super::{CheckpointFileVersion, ControlError, text::CheckpointPathText};
use crate::control::FileRange;
use peritus_patch::WorkspacePath;
use serde::{Deserialize, Serialize};

/// Exact byte interval resolved from a user-selected byte or line range at capture time.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointRange {
    selection: FileRange,
    start: u64,
    end: u64,
}
impl CheckpointRange {
    /// Binds structural selection semantics to a nonempty observed byte interval.
    ///
    /// # Errors
    /// Rejects whole-file, malformed or inconsistent byte selections.
    pub fn new(selection: FileRange, start: u64, end: u64) -> Result<Self, ControlError> {
        let value = Self { selection, start, end };
        value.validate()?;
        Ok(value)
    }
    /// Returns the original selection, so line ranges can be resolved against current bytes.
    #[must_use]
    pub const fn selection(self) -> FileRange {
        self.selection
    }
    /// Returns the exact half-open interval in the retained capture snapshot.
    #[must_use]
    pub const fn captured_interval(self) -> (u64, u64) {
        (self.start, self.end)
    }
    fn canonical_key(self) -> (u64, u64, u8, u64, u64) {
        let (kind, first, last) = match self.selection {
            FileRange::All => (0, 0, 0),
            FileRange::Bytes { start, end } => (1, start, end),
            FileRange::Lines { first, last } => (2, u64::from(first), u64::from(last)),
        };
        (self.start, self.end, kind, first, last)
    }
    const fn validate(self) -> Result<(), ControlError> {
        let valid = match self.selection {
            FileRange::Bytes { start, end } => start == self.start && end == self.end,
            FileRange::Lines { first, last } => first > 0 && first <= last,
            FileRange::All => false,
        };
        if !valid || self.start >= self.end {
            return Err(ControlError::InvalidInput);
        }
        Ok(())
    }
}

/// Closed restore capabilities; retained surrounding bytes do not grant whole-file restoration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckpointCoverage<'a> {
    /// A complete regular file and its portable mode are covered.
    WholeFile,
    /// Only these exact selected intervals are restored; surrounding current bytes are retained.
    SelectedRanges(&'a [CheckpointRange]),
    /// The target's absence is covered.
    AbsentPath,
    /// Empty-directory type and permission intent are covered.
    EmptyDirectory,
}

/// One exact covered path and its immutable checkpoint and owned-postchange versions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointPath {
    path: CheckpointPathText,
    checkpoint: CheckpointFileVersion,
    owned_postchange: Option<CheckpointFileVersion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    coverage_schema: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ranges: Option<Vec<CheckpointRange>>,
}
impl CheckpointPath {
    /// Constructs one captured path. A later owned mutation seals its expected current version.
    ///
    /// # Errors
    /// Rejects non-canonical workspace-relative paths.
    pub fn new(path: String, checkpoint: CheckpointFileVersion) -> Result<Self, ControlError> {
        let value = Self {
            path: CheckpointPathText::new(path)?,
            checkpoint,
            owned_postchange: None,
            coverage_schema: if matches!(checkpoint, CheckpointFileVersion::EmptyDirectory { .. }) {
                Some(2)
            } else {
                None
            },
            ranges: None,
        };
        value.validate()?;
        Ok(value)
    }
    /// Captures only selected ranges while retaining an exact full-source integrity identity.
    ///
    /// # Errors
    /// Rejects absent/non-file sources, empty range coverage, or out-of-source intervals.
    pub fn selected_ranges(
        path: String,
        checkpoint: CheckpointFileVersion,
        mut ranges: Vec<CheckpointRange>,
    ) -> Result<Self, ControlError> {
        ranges.sort_by_key(|range| range.canonical_key());
        ranges.dedup();
        let mut value = Self::new(path, checkpoint)?;
        value.coverage_schema = Some(2);
        value.ranges = Some(ranges);
        value.validate()?;
        Ok(value)
    }
    /// Returns the typed restoration capability represented by this immutable path manifest.
    #[must_use]
    pub fn coverage(&self) -> CheckpointCoverage<'_> {
        if let Some(ranges) = &self.ranges {
            return CheckpointCoverage::SelectedRanges(ranges);
        }
        match self.checkpoint {
            CheckpointFileVersion::Absent => CheckpointCoverage::AbsentPath,
            CheckpointFileVersion::Present { .. } => CheckpointCoverage::WholeFile,
            CheckpointFileVersion::EmptyDirectory { .. } => CheckpointCoverage::EmptyDirectory,
        }
    }
    /// Borrows the canonical workspace-relative path.
    #[must_use]
    pub fn path(&self) -> &str {
        self.path.as_str()
    }
    /// Returns the stable exact path identity used by paged snapshot coverage.
    /// A version or page position does not change the identity of the covered target.
    #[must_use]
    pub fn path_id(&self) -> peritus_types::Sha256Digest {
        use sha2::{Digest as _, Sha256};
        let mut digest = Sha256::new();
        digest.update(b"peritus-checkpoint-path-v1\0");
        digest.update(self.path().as_bytes());
        peritus_types::Sha256Digest::new(digest.finalize().into())
    }
    /// Returns the version restored by rewind.
    #[must_use]
    pub const fn checkpoint(&self) -> CheckpointFileVersion {
        self.checkpoint
    }
    /// Returns the last version observed at a completed owned execution boundary.
    #[must_use]
    pub const fn owned_postchange(&self) -> Option<CheckpointFileVersion> {
        self.owned_postchange
    }
    pub(super) fn seal(&mut self, version: CheckpointFileVersion) -> Result<(), ControlError> {
        version.validate()?;
        self.owned_postchange = Some(version);
        self.coverage_schema = if self.enhanced() { Some(2) } else { None };
        Ok(())
    }
    pub(super) const fn unseal(&mut self) {
        self.owned_postchange = None;
        self.coverage_schema = if self.enhanced() { Some(2) } else { None };
    }
    const fn enhanced(&self) -> bool {
        self.ranges.is_some()
            || matches!(self.checkpoint, CheckpointFileVersion::EmptyDirectory { .. })
            || matches!(self.owned_postchange, Some(CheckpointFileVersion::EmptyDirectory { .. }))
    }
    pub(super) fn validate(&self) -> Result<(), ControlError> {
        WorkspacePath::new(self.path()).map_err(|_| ControlError::InvalidInput)?;
        self.checkpoint.validate()?;
        if let Some(version) = self.owned_postchange {
            version.validate()?;
        }
        if self.coverage_schema != if self.enhanced() { Some(2) } else { None } {
            return Err(ControlError::UnsupportedSchema);
        }
        if let Some(ranges) = &self.ranges {
            let bytes = self.checkpoint.bytes().ok_or(ControlError::InvalidInput)?;
            if ranges.is_empty()
                || ranges.windows(2).any(|pair| pair[0].canonical_key() >= pair[1].canonical_key())
            {
                return Err(ControlError::InvalidInput);
            }
            for range in ranges {
                range.validate()?;
                if range.end > bytes {
                    return Err(ControlError::InvalidInput);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
