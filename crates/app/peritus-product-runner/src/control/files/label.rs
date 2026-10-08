//! Inert imported-source labels, distinct from host filesystem authority.

use super::{ControlError, ControlText};
use serde::{Deserialize, Serialize};

/// Nonblank UTF-8 display text for an imported source.
///
/// The label is never reopened as a path. Its enclosing journal and protocol frames own their
/// physical representation limits, so the semantic type adds no artificial byte ceiling.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FileSourceLabel(ControlText<{ usize::MAX }>);

impl FileSourceLabel {
    /// Validates inert display text without granting path authority.
    ///
    /// # Errors
    /// Rejects blank text and terminal control characters other than newline and tab.
    pub fn new(value: String) -> Result<Self, ControlError> {
        ControlText::new(value).map(Self)
    }

    /// Borrows the exact user-observed label.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl std::fmt::Debug for FileSourceLabel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FileSourceLabel")
            .field("bytes", &self.as_str().len())
            .finish_non_exhaustive()
    }
}
