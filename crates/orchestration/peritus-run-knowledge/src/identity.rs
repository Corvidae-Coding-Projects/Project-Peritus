//! Stable caller-supplied knowledge identities.

use crate::{KnowledgeError, KnowledgeErrorKind};
use vstd::prelude::*;

verus! {

/// Stable 128-bit identity of one logical knowledge section.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct KnowledgeSectionId([u8; 16]);

impl KnowledgeSectionId {
    /// Creates a nonzero section identity.
    ///
    /// # Errors
    ///
    /// Returns [`KnowledgeErrorKind::ZeroIdentifier`] for the all-zero representation.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, KnowledgeError> {
        let mut index = 0;
        while index < bytes.len()
            decreases bytes.len() - index,
        {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(KnowledgeError::plain(KnowledgeErrorKind::ZeroIdentifier))
    }

    /// Borrows the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] { &self.0 }
}

/// Stable 128-bit identity of one authoritative source path or input.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct KnowledgeSourceId([u8; 16]);

impl KnowledgeSourceId {
    /// Creates a nonzero source identity.
    ///
    /// # Errors
    ///
    /// Returns [`KnowledgeErrorKind::ZeroIdentifier`] for the all-zero representation.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, KnowledgeError> {
        let mut index = 0;
        while index < bytes.len()
            decreases bytes.len() - index,
        {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(KnowledgeError::plain(KnowledgeErrorKind::ZeroIdentifier))
    }

    /// Borrows the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] { &self.0 }
}

} // verus!
