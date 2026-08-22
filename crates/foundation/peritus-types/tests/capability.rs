//! Boundary and negative tests for the capability-name grammar.

use peritus_types::{CapabilityName, CapabilityNameError};

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
