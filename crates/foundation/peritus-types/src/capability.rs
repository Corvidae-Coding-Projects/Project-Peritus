//! Validated, hierarchical capability names.

// Range methods, comparison matches, and direct `String::as_bytes` either obscure the recursive
// grammar proof or lack a pinned-vstd specification. Keep the equivalent primitive checks.
#![allow(clippy::comparison_chain, clippy::manual_range_contains, clippy::redundant_as_str)]

use crate::CapabilityNameError;
use std::cmp::Ordering;

fn canonical_byte_order_from_exec(left: &[u8], right: &[u8], index: usize) -> Ordering {
    if index == left.len() {
        if index == right.len() { Ordering::Equal } else { Ordering::Less }
    } else if index == right.len() {
        Ordering::Greater
    } else if left[index] < right[index] {
        Ordering::Less
    } else if left[index] > right[index] {
        Ordering::Greater
    } else {
        canonical_byte_order_from_exec(left, right, index + 1)
    }
}
const fn is_ascii_lowercase(byte: u8) -> bool {
    b'a' <= byte && byte <= b'z'
}
const fn is_capability_tail(byte: u8) -> bool {
    is_ascii_lowercase(byte) || (b'0' <= byte && byte <= b'9') || byte == b'-'
}
fn validate_from(
    bytes: &[u8],
    index: usize,
    at_segment_start: bool,
) -> Result<(), CapabilityNameError> {
    if index > bytes.len() {
        Err(CapabilityNameError::EmptySegment)
    } else if index == bytes.len() {
        if at_segment_start {
            if index == 0 {
                Err(CapabilityNameError::Empty)
            } else {
                Err(CapabilityNameError::EmptySegment)
            }
        } else {
            Ok(())
        }
    } else {
        let byte = bytes[index];
        if at_segment_start {
            if is_ascii_lowercase(byte) {
                validate_from(bytes, index + 1, false)
            } else if byte == b'.' {
                Err(CapabilityNameError::EmptySegment)
            } else {
                Err(CapabilityNameError::InvalidSegmentStart)
            }
        } else if byte == b'.' {
            validate_from(bytes, index + 1, true)
        } else if is_capability_tail(byte) {
            validate_from(bytes, index + 1, false)
        } else {
            Err(CapabilityNameError::InvalidCharacter)
        }
    }
}
fn validate_capability_bytes(bytes: &[u8]) -> Result<(), CapabilityNameError> {
    if bytes.len() > CapabilityName::MAX_LENGTH {
        Err(CapabilityNameError::TooLong)
    } else {
        validate_from(bytes, 0, true)
    }
}
#[doc = " A validated hierarchical name used to identify a capability class."]
#[doc = ""]
#[doc = " Names contain at most 128 ASCII bytes and match"]
#[doc = " `[a-z][a-z0-9-]*(.[a-z][a-z0-9-]*)*`. A dot separates nonempty segments; it"]
#[doc = " is not a wildcard and carries no authority semantics by itself."]
#[derive(Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CapabilityName {
    value: String,
}
impl Clone for CapabilityName {
    fn clone(&self) -> Self {
        Self { value: self.value.clone() }
    }
}
impl CapabilityName {
    #[doc = " The maximum accepted UTF-8 byte length."]
    pub const MAX_LENGTH: usize = 128;
    #[doc = " Creates a capability name after complete grammar validation."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns the precise [`CapabilityNameError`] category when the length or grammar is invalid."]
    pub fn new(value: String) -> Result<Self, CapabilityNameError> {
        let validation = validate_capability_bytes(value.as_str().as_bytes());
        match validation {
            Ok(()) => Ok(Self { value }),
            Err(error) => Err(error),
        }
    }
    #[doc = " Borrows the validated name."]
    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.value.as_str()
    }
    #[doc = " Compares two names by their exact validated ASCII bytes."]
    #[doc = ""]
    #[doc = " This order is canonical and carries no hierarchical authority semantics."]
    #[must_use]
    pub fn canonical_cmp(&self, other: &Self) -> Ordering {
        canonical_byte_order_from_exec(
            self.value.as_str().as_bytes(),
            other.value.as_str().as_bytes(),
            0,
        )
    }
    #[doc = " Consumes the name and returns its validated string."]
    #[must_use]
    pub fn into_string(self) -> String {
        self.value
    }
}
