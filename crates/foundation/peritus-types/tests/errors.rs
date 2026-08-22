//! Stability tests for public diagnostic codes.

use peritus_types::{
    CapabilityNameError, IdentifierError, OneBasedNumberError, ResourceQuantityError,
};

#[test]
fn error_codes_are_stable_and_category_specific() {
    assert_eq!(IdentifierError::Zero.code(), "PERITUS-TYPES-ID-001");
    assert_eq!(OneBasedNumberError::Zero.code(), "PERITUS-TYPES-NUMBER-001");
    assert_eq!(OneBasedNumberError::Overflow.code(), "PERITUS-TYPES-NUMBER-002");
    assert_eq!(CapabilityNameError::Empty.code(), "PERITUS-TYPES-CAPABILITY-001");
    assert_eq!(CapabilityNameError::TooLong.code(), "PERITUS-TYPES-CAPABILITY-002");
    assert_eq!(CapabilityNameError::EmptySegment.code(), "PERITUS-TYPES-CAPABILITY-003");
    assert_eq!(CapabilityNameError::InvalidSegmentStart.code(), "PERITUS-TYPES-CAPABILITY-004");
    assert_eq!(CapabilityNameError::InvalidCharacter.code(), "PERITUS-TYPES-CAPABILITY-005");
    assert_eq!(ResourceQuantityError::Overflow.code(), "PERITUS-TYPES-RESOURCE-001");
    assert_eq!(ResourceQuantityError::Underflow.code(), "PERITUS-TYPES-RESOURCE-002");
}
