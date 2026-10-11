//! Stable caller-supplied knowledge identities.

mod collection;
pub use collection::validate_section_ids;

use crate::{KnowledgeError, KnowledgeErrorKind};
use core::cmp::Ordering;

#[doc = " Stable 128-bit identity of one logical knowledge section."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct KnowledgeSectionId([u8; 16]);
impl KnowledgeSectionId {
    #[doc = " Creates a nonzero section identity."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`KnowledgeErrorKind::ZeroIdentifier`] for the all-zero representation."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, KnowledgeError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(KnowledgeError::plain(KnowledgeErrorKind::ZeroIdentifier))
    }
    #[doc = " Borrows the exact identity bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
    #[doc = " Returns whether every identity byte is equal."]
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        bytes_equal(self.as_bytes(), other.as_bytes())
    }
    pub(crate) fn canonical_order(&self, other: &Self) -> Ordering {
        compare_bytes_from(self.as_bytes(), other.as_bytes(), 0)
    }
}
#[doc = " Stable 128-bit identity of one authoritative source path or input."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct KnowledgeSourceId([u8; 16]);
impl KnowledgeSourceId {
    pub(crate) fn canonical_order(&self, other: &Self) -> Ordering {
        compare_bytes_from(self.as_bytes(), other.as_bytes(), 0)
    }
    #[doc = " Creates a nonzero source identity."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`KnowledgeErrorKind::ZeroIdentifier`] for the all-zero representation."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, KnowledgeError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(KnowledgeError::plain(KnowledgeErrorKind::ZeroIdentifier))
    }
    #[doc = " Borrows the exact identity bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
    #[doc = " Returns whether every identity byte is equal."]
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        bytes_equal(self.as_bytes(), other.as_bytes())
    }
}
#[doc = " Returns whether two fixed-size byte arrays are identical."]
pub fn bytes_equal<const N: usize>(left: &[u8; N], right: &[u8; N]) -> bool {
    let mut index = 0;
    while index < N {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}
fn compare_bytes_from<const N: usize>(left: &[u8; N], right: &[u8; N], index: usize) -> Ordering {
    if index == N {
        Ordering::Equal
    } else if left[index] < right[index] {
        Ordering::Less
    } else if left[index] > right[index] {
        Ordering::Greater
    } else {
        compare_bytes_from(left, right, index + 1)
    }
}
