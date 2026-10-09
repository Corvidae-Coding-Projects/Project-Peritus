//! Inert canonical path/range descriptors. The accepting host owns all read authorization.

use super::{ControlError, ControlText, Sha256Digest};
use peritus_patch::WorkspacePath;
use serde::Deserialize;
use serde::Serialize;

pub(super) const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;

/// Explicit future-read preference. Refresh is supported only for selected-workspace sources.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileMode {
    /// Keep the confirmed immutable bytes for future requests.
    Snapshot,
    /// Reauthorize and observe this exact workspace path at each request admission.
    RefreshOnRequest,
}

/// Explicit source range, retained separately from the byte range resolved during observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum FileRange {
    /// Complete bounded source, with no silent truncation.
    All,
    /// Exact nonempty half-open byte interval.
    Bytes {
        /// Included first byte offset.
        start: u64,
        /// Excluded ending byte offset.
        end: u64,
    },
    /// One-based inclusive complete lines, retaining original terminators.
    Lines {
        /// Included first line number.
        first: u32,
        /// Included last line number.
        last: u32,
    },
}
impl FileRange {
    /// Checks structural bounds; source existence and resolved range require an actual read.
    ///
    /// # Errors
    /// Rejects empty/reversed intervals, zero line numbers or bounds beyond the source ceiling.
    pub const fn validate(self) -> Result<(), ControlError> {
        match self {
            Self::All => Ok(()),
            Self::Bytes { start, end } if start < end && end <= MAX_SOURCE_BYTES => Ok(()),
            Self::Lines { first, last }
                if first > 0 && first <= last && last as u64 <= MAX_SOURCE_BYTES =>
            {
                Ok(())
            }
            _ => Err(ControlError::InvalidInput),
        }
    }
}

/// Source selected by the user, without a reusable directory handle or ambient read grant.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileSource {
    origin: Origin,
    range: FileRange,
    mode: FileMode,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
enum Origin {
    Workspace { folder: [u8; 32], path: ControlText<4096> },
    Import { label: ControlText<1024> },
}
impl FileSource {
    /// Describes one exact selected-workspace path; the conversation supplies workspace ID.
    ///
    /// # Errors
    /// Rejects a noncanonical/protected path or malformed explicit range.
    pub fn workspace(
        folder: Sha256Digest,
        path: &WorkspacePath,
        range: FileRange,
        mode: FileMode,
    ) -> Result<Self, ControlError> {
        let value = Self {
            origin: Origin::Workspace {
                folder: folder.into_bytes(),
                path: ControlText::new(path.as_str().to_owned())?,
            },
            range,
            mode,
        };
        value.validate()?;
        Ok(value)
    }
    /// Describes an explicitly imported snapshot. Its label is never reopened as a path.
    ///
    /// # Errors
    /// Rejects malformed ranges; caller still must validate and confirm imported bytes.
    pub fn imported(label: ControlText<1024>, range: FileRange) -> Result<Self, ControlError> {
        let value = Self { origin: Origin::Import { label }, range, mode: FileMode::Snapshot };
        value.validate()?;
        Ok(value)
    }
    /// Returns the observed folder identity, absent for an inert external import.
    #[must_use]
    pub const fn folder(&self) -> Option<Sha256Digest> {
        match &self.origin {
            Origin::Workspace { folder, .. } => Some(Sha256Digest::new(*folder)),
            Origin::Import { .. } => None,
        }
    }
    /// Borrows the exact relative workspace path, never an external-import label.
    #[must_use]
    pub fn path(&self) -> Option<&str> {
        match &self.origin {
            Origin::Workspace { path, .. } => Some(path.as_str()),
            Origin::Import { .. } => None,
        }
    }
    /// Borrows the inert user-visible path or source label.
    #[must_use]
    pub fn label(&self) -> &str {
        match &self.origin {
            Origin::Workspace { path, .. } => path.as_str(),
            Origin::Import { label } => label.as_str(),
        }
    }
    /// Returns explicit range semantics, which may resolve to different bytes after refresh.
    #[must_use]
    pub const fn range(&self) -> FileRange {
        self.range
    }
    /// Returns the original user-confirmed future-read preference.
    #[must_use]
    pub const fn mode(&self) -> FileMode {
        self.mode
    }
    pub(super) fn validate(&self) -> Result<(), ControlError> {
        self.range.validate()?;
        match &self.origin {
            Origin::Workspace { path, .. } => {
                WorkspacePath::new(path.as_str()).map_err(|_| ControlError::InvalidInput)?;
            }
            Origin::Import { .. } if self.mode != FileMode::Snapshot => {
                return Err(ControlError::InvalidInput);
            }
            Origin::Import { .. } => {}
        }
        Ok(())
    }
}
