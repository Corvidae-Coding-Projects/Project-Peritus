//! Inert canonical path/range descriptors. The accepting host owns all read authorization.

use super::{ControlError, ControlText, Sha256Digest};
use peritus_patch::WorkspacePath;
use serde::Deserialize;
use serde::Serialize;

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
    /// Complete source, with no silent truncation.
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
    /// Rejects empty/reversed intervals or zero line numbers.
    pub const fn validate(self) -> Result<(), ControlError> {
        match self {
            Self::All => Ok(()),
            Self::Bytes { start, end } if start < end => Ok(()),
            Self::Lines { first, last } if first > 0 && first <= last => Ok(()),
            _ => Err(ControlError::InvalidInput),
        }
    }
}

/// Source selected by the user, without a reusable directory handle or ambient read grant.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FileSource {
    origin: Origin,
    range: FileRange,
    mode: FileMode,
}
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
enum Origin {
    Workspace { folder: [u8; 32], path: String },
    NativeWorkspace { folder: [u8; 32], path: String, platform: u8 },
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
        let origin = if legacy_source_path(path) {
            Origin::Workspace { folder: folder.into_bytes(), path: path.as_str().to_owned() }
        } else {
            Origin::NativeWorkspace {
                folder: folder.into_bytes(),
                path: path.as_str().to_owned(),
                platform: native_platform_tag(),
            }
        };
        let value = Self { origin, range, mode };
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
            Origin::Workspace { folder, .. } | Origin::NativeWorkspace { folder, .. } => {
                Some(Sha256Digest::new(*folder))
            }
            Origin::Import { .. } => None,
        }
    }
    /// Borrows the exact relative workspace path, never an external-import label.
    #[must_use]
    pub const fn path(&self) -> Option<&str> {
        match &self.origin {
            Origin::Workspace { path, .. } | Origin::NativeWorkspace { path, .. } => {
                Some(path.as_str())
            }
            Origin::Import { .. } => None,
        }
    }
    /// Borrows the inert user-visible path or source label.
    #[must_use]
    pub fn label(&self) -> &str {
        match &self.origin {
            Origin::Workspace { path, .. } | Origin::NativeWorkspace { path, .. } => path.as_str(),
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
                let path =
                    WorkspacePath::new(path.as_str()).map_err(|_| ControlError::InvalidInput)?;
                if !legacy_source_path(&path) {
                    return Err(ControlError::InvalidInput);
                }
            }
            Origin::NativeWorkspace { path, platform, .. } => {
                let path =
                    WorkspacePath::new(path.as_str()).map_err(|_| ControlError::InvalidInput)?;
                if legacy_source_path(&path) || *platform != native_platform_tag() {
                    return Err(ControlError::InvalidInput);
                }
            }
            Origin::Import { .. } if self.mode != FileMode::Snapshot => {
                return Err(ControlError::InvalidInput);
            }
            Origin::Import { .. } => {}
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for FileSource {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct SourceWire {
            origin: Origin,
            range: FileRange,
            mode: FileMode,
        }
        let wire = SourceWire::deserialize(deserializer)?;
        let value = Self { origin: wire.origin, range: wire.range, mode: wire.mode };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

impl std::fmt::Debug for Origin {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Workspace { path, .. } | Self::NativeWorkspace { path, .. } => formatter
                .debug_struct("WorkspaceFileSource")
                .field("path_bytes", &path.len())
                .finish_non_exhaustive(),
            Self::Import { label } => {
                formatter.debug_struct("Import").field("label", label).finish()
            }
        }
    }
}

// Historical source records also used ControlText, whose Unicode control/blank rules were
// stricter than WorkspacePath. Classify those old bytes exactly without limiting new sources.
fn legacy_source_path(path: &WorkspacePath) -> bool {
    !path.requires_extended_encoding()
        && !path.as_str().trim().is_empty()
        && !path.as_str().chars().any(|ch| ch.is_control() && ch != '\n' && ch != '\t')
}

const fn native_platform_tag() -> u8 {
    #[cfg(unix)]
    let tag = 1;
    #[cfg(windows)]
    let tag = 2;
    #[cfg(not(any(unix, windows)))]
    let tag = 3;
    tag
}
