//! Checked checkpoint metadata and native paths, independent of manifest page size.

use super::ControlError;
use peritus_patch::WorkspacePath;
use serde::{Deserialize, Serialize};

/// Nonempty inert checkpoint metadata. Storage pages do not limit its logical byte length.
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct CheckpointText(String);

/// Concrete borrowed iterator over exact checkpoint metadata.
pub type CheckpointTextIter<'a> =
    std::iter::Map<std::slice::Iter<'a, CheckpointText>, fn(&'a CheckpointText) -> &'a str>;

impl CheckpointText {
    /// Checks text without substituting a page or field allowance for its logical length.
    ///
    /// # Errors
    /// Rejects empty text and terminal control characters other than newline and tab.
    pub fn new(text: String) -> Result<Self, ControlError> {
        if text.trim().is_empty()
            || text.chars().any(|ch| ch.is_control() && ch != '\n' && ch != '\t')
        {
            return Err(ControlError::InvalidInput);
        }
        Ok(Self(text))
    }

    /// Borrows exact metadata; display surfaces must sanitize it for their own context.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for CheckpointText {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Debug for CheckpointText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckpointText").field("bytes", &self.0.len()).finish_non_exhaustive()
    }
}

#[derive(Clone, Eq, PartialEq, Serialize)]
pub(super) struct CheckpointPathText(String);

impl CheckpointPathText {
    pub(super) fn new(path: String) -> Result<Self, ControlError> {
        WorkspacePath::new(&path).map_err(|_| ControlError::InvalidInput)?;
        Ok(Self(path))
    }

    pub(super) fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for CheckpointPathText {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Debug for CheckpointPathText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckpointPathText").field("bytes", &self.0.len()).finish_non_exhaustive()
    }
}
