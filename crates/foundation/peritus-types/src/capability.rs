//! Validated, hierarchical capability names.

// Range methods, comparison matches, and direct `String::as_bytes` either obscure the recursive
// grammar proof or lack a pinned-vstd specification. Keep the equivalent primitive checks.
#![allow(
    clippy::comparison_chain,
    clippy::manual_range_contains,
    clippy::redundant_as_str
)]

use crate::CapabilityNameError;
use vstd::prelude::*;
#[cfg(verus_only)]
use vstd::utf8::encode_utf8;

verus! {

spec fn ascii_lowercase(byte: u8) -> bool {
    b'a' <= byte && byte <= b'z'
}

spec fn capability_tail(byte: u8) -> bool {
    ascii_lowercase(byte) || (b'0' <= byte && byte <= b'9') || byte == b'-'
}

spec fn valid_from(bytes: Seq<u8>, index: nat, at_segment_start: bool) -> bool
    decreases bytes.len() - index,
{
    if index >= bytes.len() {
        index == bytes.len() && !at_segment_start
    } else {
        let byte = bytes[index as int];
        if at_segment_start {
            ascii_lowercase(byte) && valid_from(bytes, index + 1, false)
        } else if byte == b'.' {
            valid_from(bytes, index + 1, true)
        } else {
            capability_tail(byte) && valid_from(bytes, index + 1, false)
        }
    }
}

/// Returns whether bytes match the complete capability-name grammar and length bound.
pub closed spec fn valid_capability_bytes(bytes: Seq<u8>) -> bool {
    bytes.len() <= CapabilityName::MAX_LENGTH && valid_from(bytes, 0, true)
}

const fn is_ascii_lowercase(byte: u8) -> (result: bool)
    ensures
        result == ascii_lowercase(byte),
{
    b'a' <= byte && byte <= b'z'
}

const fn is_capability_tail(byte: u8) -> (result: bool)
    ensures
        result == capability_tail(byte),
{
    is_ascii_lowercase(byte) || (b'0' <= byte && byte <= b'9') || byte == b'-'
}

fn validate_from(
    bytes: &[u8],
    index: usize,
    at_segment_start: bool,
) -> (result: Result<(), CapabilityNameError>)
    ensures
        result.is_ok() == valid_from(bytes@, index as nat, at_segment_start),
    decreases bytes.len() - index,
{
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

fn validate_capability_bytes(bytes: &[u8]) -> (result: Result<(), CapabilityNameError>)
    ensures
        result.is_ok() == valid_capability_bytes(bytes@),
{
    if bytes.len() > CapabilityName::MAX_LENGTH {
        Err(CapabilityNameError::TooLong)
    } else {
        validate_from(bytes, 0, true)
    }
}

/// A validated hierarchical name used to identify a capability class.
///
/// Names contain at most 128 ASCII bytes and match
/// `[a-z][a-z0-9-]*(.[a-z][a-z0-9-]*)*`. A dot separates nonempty segments; it
/// is not a wildcard and carries no authority semantics by itself.
#[derive(Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CapabilityName {
    value: String,
}

impl Clone for CapabilityName {
    fn clone(&self) -> (result: Self)
        ensures
            result.spec_value() == self.spec_value(),
    {
        proof {
            use_type_invariant(&*self);
        }
        Self { value: self.value.clone() }
    }
}

impl CapabilityName {
    /// The maximum accepted UTF-8 byte length.
    pub const MAX_LENGTH: usize = 128;

    #[verifier::type_invariant]
    closed spec fn invariant(&self) -> bool {
        valid_capability_bytes(encode_utf8(self.value@))
    }

    /// Creates a capability name after complete grammar validation.
    ///
    /// # Errors
    ///
    /// Returns the precise [`CapabilityNameError`] category when the length or grammar is invalid.
    pub fn new(value: String) -> (result: Result<Self, CapabilityNameError>)
        ensures
            result.is_ok() == valid_capability_bytes(encode_utf8(value@)),
            match result {
                Ok(name) => name.spec_value() == value@,
                Err(_) => true,
            },
    {
        let validation = validate_capability_bytes(value.as_str().as_bytes());
        match validation {
            Ok(()) => Ok(Self { value }),
            Err(error) => Err(error),
        }
    }

    /// Returns the character-sequence view used by specifications.
    pub closed spec fn spec_value(&self) -> Seq<char> {
        self.value@
    }

    /// Returns whether the stored value satisfies the complete grammar.
    pub closed spec fn is_valid(&self) -> bool {
        valid_capability_bytes(encode_utf8(self.spec_value()))
    }

    /// Borrows the validated name.
    #[must_use]
    pub const fn as_str(&self) -> (value: &str)
        ensures
            value@ == self.spec_value(),
    {
        self.value.as_str()
    }

    /// Consumes the name and returns its validated string.
    #[must_use]
    pub fn into_string(self) -> (value: String)
        ensures
            value@ == self.spec_value(),
    {
        self.value
    }

}

} // verus!
