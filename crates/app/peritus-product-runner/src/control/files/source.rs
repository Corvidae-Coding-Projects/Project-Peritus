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
    /// Rejects empty/reversed intervals and zero or reversed line numbers.
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
    Import { label: ControlText<4096> },
    UserMessage,
    AcceptedProposal { reply: crate::control::PublicReplyReference },
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
    pub fn imported(label: ControlText<4096>, range: FileRange) -> Result<Self, ControlError> {
        let value = Self { origin: Origin::Import { label }, range, mode: FileMode::Snapshot };
        value.validate()?;
        Ok(value)
    }
    /// Describes a complete immutable user-authored message, rather than attachment source data.
    #[must_use]
    pub const fn user_message() -> Self {
        Self { origin: Origin::UserMessage, range: FileRange::All, mode: FileMode::Snapshot }
    }
    /// Retains immutable model authorship when a user explicitly accepts a proposal.
    #[must_use]
    pub const fn accepted_proposal(reply: crate::control::PublicReplyReference) -> Self {
        Self {
            origin: Origin::AcceptedProposal { reply },
            range: FileRange::All,
            mode: FileMode::Snapshot,
        }
    }
    /// Borrows the exact model-authored proposal explicitly accepted by the user.
    #[must_use]
    pub const fn proposal(&self) -> Option<&crate::control::PublicReplyReference> {
        match &self.origin {
            Origin::AcceptedProposal { reply } => Some(reply),
            _ => None,
        }
    }
    /// Reports whether this source carries user-authored or explicitly user-confirmed instructions.
    #[must_use]
    pub const fn is_user_instruction(&self) -> bool {
        matches!(self.origin, Origin::UserMessage | Origin::AcceptedProposal { .. })
    }
    /// Reports whether these exact bytes were explicitly admitted as a user message.
    #[must_use]
    pub const fn is_user_message(&self) -> bool {
        matches!(self.origin, Origin::UserMessage)
    }
    /// Returns the observed folder identity, absent for an inert external import.
    #[must_use]
    pub const fn folder(&self) -> Option<Sha256Digest> {
        match &self.origin {
            Origin::Workspace { folder, .. } => Some(Sha256Digest::new(*folder)),
            Origin::Import { .. } | Origin::UserMessage | Origin::AcceptedProposal { .. } => None,
        }
    }
    /// Borrows the exact relative workspace path, never an external-import label.
    #[must_use]
    pub fn path(&self) -> Option<&str> {
        match &self.origin {
            Origin::Workspace { path, .. } => Some(path.as_str()),
            Origin::Import { .. } | Origin::UserMessage | Origin::AcceptedProposal { .. } => None,
        }
    }
    /// Borrows the inert user-visible path or source label.
    #[must_use]
    pub fn label(&self) -> &str {
        match &self.origin {
            Origin::Workspace { path, .. } => path.as_str(),
            Origin::Import { label } => label.as_str(),
            Origin::UserMessage => "User message",
            Origin::AcceptedProposal { .. } => "User-confirmed agent proposal",
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
            Origin::UserMessage | Origin::AcceptedProposal { .. }
                if self.mode != FileMode::Snapshot || self.range != FileRange::All =>
            {
                return Err(ControlError::InvalidInput);
            }
            Origin::Import { .. } | Origin::UserMessage | Origin::AcceptedProposal { .. } => {}
        }
        Ok(())
    }
}
