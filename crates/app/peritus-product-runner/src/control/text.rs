//! Bounded inert public text with redacted Debug output.

use super::ControlError;
use serde::Deserialize;
use serde::Serialize;

/// Nonempty UTF-8 control text with a compile-time byte ceiling.
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct ControlText<const MAX: usize>(String);

/// Concrete borrowed text iterator used by ordinary-safe control APIs.
pub type ControlTextIter<'a, const MAX: usize> =
    std::iter::Map<std::slice::Iter<'a, ControlText<MAX>>, fn(&'a ControlText<MAX>) -> &'a str>;

impl<const MAX: usize> ControlText<MAX> {
    /// Validates nonempty inert text and its UTF-8 byte ceiling.
    ///
    /// # Errors
    /// Rejects empty/oversized text and terminal control characters except newline and tab.
    pub fn new(text: String) -> Result<Self, ControlError> {
        if text.trim().is_empty()
            || text.len() > MAX
            || text.chars().any(|ch| ch.is_control() && ch != '\n' && ch != '\t')
        {
            return Err(ControlError::InvalidInput);
        }
        Ok(Self(text))
    }
    /// Borrows exact user-approved text; callers must sanitize for their display surface.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de, const MAX: usize> Deserialize<'de> for ControlText<MAX> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::new(text).map_err(serde::de::Error::custom)
    }
}

impl<const MAX: usize> TryFrom<String> for ControlText<MAX> {
    type Error = ControlError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::new(text)
    }
}
impl<const MAX: usize> std::fmt::Debug for ControlText<MAX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ControlText").field("bytes", &self.0.len()).finish_non_exhaustive()
    }
}
