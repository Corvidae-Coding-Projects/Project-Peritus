use peritus_conformance::{
    CaseId, FailureCode, FailureCodeError, IdentifierError, ObservationId, ObservationValue,
    ReportText, ReportTextError, SuiteId,
};

#[test]
fn path_identifiers_accept_canonical_boundaries() {
    for value in ["a", "suite.case-2", "z9"] {
        assert_eq!(SuiteId::new(value).expect("valid suite ID").as_str(), value);
        assert_eq!(CaseId::new(value).expect("valid case ID").as_str(), value);
        assert_eq!(ObservationId::new(value).expect("valid observation ID").as_str(), value);
    }
    let maximum = format!("a{}", "0".repeat(SuiteId::MAX_LENGTH - 1));
    assert_eq!(SuiteId::new(&maximum).expect("maximum ID").as_str(), maximum);
}

#[test]
fn path_identifiers_reject_every_malformed_category() {
    let too_long = format!("a{}", "0".repeat(SuiteId::MAX_LENGTH));
    for (value, expected) in [
        ("", IdentifierError::Empty),
        (too_long.as_str(), IdentifierError::TooLong),
        ("a..b", IdentifierError::EmptySegment),
        ("a.", IdentifierError::EmptySegment),
        ("a.2b", IdentifierError::InvalidSegmentStart),
        ("A", IdentifierError::InvalidSegmentStart),
        ("a_b", IdentifierError::InvalidCharacter),
        ("a/b", IdentifierError::InvalidCharacter),
        ("é", IdentifierError::InvalidSegmentStart),
    ] {
        assert_eq!(SuiteId::new(value), Err(expected), "value {value:?}");
    }
}

#[test]
fn failure_codes_are_validated_and_have_stable_error_codes() {
    assert_eq!(
        FailureCode::new("PERITUS-CONFORMANCE-CASE-7").expect("valid failure code").as_str(),
        "PERITUS-CONFORMANCE-CASE-7"
    );
    let too_long = format!("A{}", "B".repeat(FailureCode::MAX_LENGTH));
    for (value, expected) in [
        ("", FailureCodeError::Empty),
        (too_long.as_str(), FailureCodeError::TooLong),
        ("1CODE", FailureCodeError::InvalidStart),
        ("code", FailureCodeError::InvalidStart),
        ("CODE-", FailureCodeError::InvalidCharacter),
        ("CODE--BAD", FailureCodeError::InvalidCharacter),
        ("CODE_BAD", FailureCodeError::InvalidCharacter),
    ] {
        assert_eq!(FailureCode::new(value), Err(expected), "value {value:?}");
        assert!(expected.code().starts_with("PERITUS-CONFORMANCE-CODE-"));
    }
}

#[test]
fn identifier_error_diagnostics_are_stable() {
    let categories = [
        IdentifierError::Empty,
        IdentifierError::TooLong,
        IdentifierError::EmptySegment,
        IdentifierError::InvalidSegmentStart,
        IdentifierError::InvalidCharacter,
    ];
    for category in categories {
        assert!(category.code().starts_with("PERITUS-CONFORMANCE-ID-"));
        assert!(!category.to_string().is_empty());
    }
}

#[test]
fn report_text_rejects_empty_and_oversized_values_without_truncation() {
    assert_eq!(ReportText::new(""), Err(ReportTextError::Empty));
    let maximum = "é".repeat(ReportText::MAX_LENGTH / 2);
    let text = ReportText::new(&maximum).expect("exact byte maximum must be valid");
    assert_eq!(text.as_str(), maximum);
    assert_eq!(text.into_string(), maximum);

    let oversized = format!("{maximum}a");
    assert_eq!(ReportText::new(oversized), Err(ReportTextError::TooLong));
    assert_eq!(ReportTextError::Empty.code(), "PERITUS-CONFORMANCE-TEXT-001");
    assert_eq!(ReportTextError::TooLong.code(), "PERITUS-CONFORMANCE-TEXT-002");
}

#[test]
fn digest_observations_preserve_exact_bytes_without_hashing_claims() {
    let bytes = [0xa5; 32];
    assert_eq!(ObservationValue::from(bytes), ObservationValue::Digest(bytes));
}
