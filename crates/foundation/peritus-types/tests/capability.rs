//! Boundary and negative tests for the capability-name grammar.

use peritus_types::{CapabilityName, CapabilityNameError};
use std::cmp::Ordering;

fn validate(value: &str) -> Result<CapabilityName, CapabilityNameError> {
    CapabilityName::new(value.to_owned())
}

#[test]
fn accepts_the_complete_grammar_and_length_boundaries() {
    for value in
        ["a", "z", "read", "read-file", "workspace.read", "workspace.read-2", "a.abc-123.z-"]
    {
        let name = validate(value).unwrap_or_else(|error| panic!("{value:?}: {error:?}"));
        assert_eq!(name.as_str(), value);
        assert_eq!(name.clone().into_string(), value);
    }

    let maximum = "a".repeat(CapabilityName::MAX_LENGTH);
    assert_eq!(validate(&maximum).expect("maximum length").as_str(), maximum);
}

#[test]
fn rejects_empty_overlong_and_empty_segments() {
    assert_eq!(validate(""), Err(CapabilityNameError::Empty));
    assert_eq!(
        validate(&"a".repeat(CapabilityName::MAX_LENGTH + 1)),
        Err(CapabilityNameError::TooLong)
    );
    for value in [".read", "read.", "read..file", "."] {
        assert_eq!(validate(value), Err(CapabilityNameError::EmptySegment), "{value:?}");
    }
}

#[test]
fn rejects_invalid_segment_starts() {
    for value in ["0read", "-read", "read.0file", "read.-file", "read.*", "Read"] {
        assert_eq!(validate(value), Err(CapabilityNameError::InvalidSegmentStart), "{value:?}");
    }
}

#[test]
fn rejects_non_ascii_and_disallowed_ascii() {
    for value in ["read_file", "read/file", "read file", "réad"] {
        assert_eq!(validate(value), Err(CapabilityNameError::InvalidCharacter), "{value:?}");
    }
}

#[test]
fn canonical_comparison_is_exact_ascii_lexicographic_order() {
    let a = validate("repo.read").expect("valid capability");
    let b = validate("repo.write").expect("valid capability");
    let prefix = validate("repo").expect("valid capability");
    let same = validate("repo.read").expect("valid capability");

    assert_eq!(a.canonical_cmp(&b), Ordering::Less);
    assert_eq!(b.canonical_cmp(&a), Ordering::Greater);
    assert_eq!(prefix.canonical_cmp(&a), Ordering::Less);
    assert_eq!(a.canonical_cmp(&same), Ordering::Equal);
    assert_eq!(a.canonical_cmp(&b), a.as_str().as_bytes().cmp(b.as_str().as_bytes()));
}
