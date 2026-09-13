//! Boundary regressions for canonical directional schema contracts and evidence.

mod support;

use peritus_obligations::{
    ObligationError, ObligationErrorKind, ObligationLimits, ObligationSpec, PublicTaskSource,
    RequirementDraft, RequirementEvidence, RequirementLedger, SchemaDirection, SchemaEvidence,
    SchemaField, SchemaFieldId, SchemaRequirement,
};
use peritus_types::Sha256Digest;

use support::{binding, candidate, ledger, requirement_id};

const fn late_digest(last: u8) -> Sha256Digest {
    let mut bytes = [0; 32];
    bytes[31] = last;
    Sha256Digest::new(bytes)
}

fn field(last: u8, name: &[u8]) -> SchemaField {
    SchemaField::new(SchemaFieldId::new(late_digest(last)), name.to_vec(), 64)
        .expect("schema field")
}

fn schema_limits(maximum_fields: usize) -> ObligationLimits {
    ObligationLimits::new(1024, 512, 16, 16, maximum_fields, 16).expect("schema limits")
}

fn error_kind<T: core::fmt::Debug>(result: Result<T, ObligationError>) -> ObligationErrorKind {
    result.expect_err("expected schema rejection").kind()
}

#[test]
fn schema_field_retains_exact_identity_and_name_bounds() {
    let empty = SchemaField::new(SchemaFieldId::new(late_digest(1)), Vec::new(), 3)
        .expect_err("empty field name");
    assert_eq!(empty.kind(), ObligationErrorKind::InvalidText);
    assert_eq!(empty.expected(), Some(3));
    assert_eq!(empty.actual(), Some(0));

    let oversized = SchemaField::new(SchemaFieldId::new(late_digest(2)), b"four".to_vec(), 3)
        .expect_err("oversized field name");
    assert_eq!(oversized.kind(), ObligationErrorKind::InvalidText);
    assert_eq!(oversized.expected(), Some(3));
    assert_eq!(oversized.actual(), Some(4));

    let exact_name = vec![0, b'X', 0xff];
    let accepted =
        SchemaField::new(SchemaFieldId::new(late_digest(3)), exact_name.clone(), exact_name.len())
            .expect("bounded binary field name");
    assert_eq!(accepted.id(), SchemaFieldId::new(late_digest(3)));
    assert_eq!(accepted.exact_name(), exact_name);
    assert_eq!(accepted.clone(), accepted);
}

#[test]
fn requirement_constructor_enforces_full_identity_order_and_error_priority() {
    let limits = schema_limits(2);
    let empty = SchemaRequirement::new(SchemaDirection::Request, Vec::new(), limits)
        .expect_err("empty requirement");
    assert_eq!(empty.kind(), ObligationErrorKind::InvalidSchema);
    assert_eq!(empty.expected(), Some(2));
    assert_eq!(empty.actual(), Some(0));

    let oversized = SchemaRequirement::new(
        SchemaDirection::Request,
        vec![field(3, b"c"), field(2, b"b"), field(2, b"duplicate")],
        limits,
    )
    .expect_err("size limit precedes pair validation");
    assert_eq!(oversized.kind(), ObligationErrorKind::InvalidSchema);
    assert_eq!(oversized.expected(), Some(2));
    assert_eq!(oversized.actual(), Some(3));

    assert_eq!(
        error_kind(SchemaRequirement::new(
            SchemaDirection::Request,
            vec![field(1, b"first"), field(1, b"duplicate")],
            limits,
        )),
        ObligationErrorKind::DuplicateValue,
    );
    assert_eq!(
        error_kind(SchemaRequirement::new(
            SchemaDirection::Request,
            vec![field(2, b"later"), field(1, b"earlier")],
            limits,
        )),
        ObligationErrorKind::NonCanonicalOrder,
    );

    let exact = SchemaRequirement::new(
        SchemaDirection::Response,
        vec![field(1, b"one"), field(2, b"two")],
        limits,
    )
    .expect("late-byte canonical order");
    assert_eq!(exact.direction(), SchemaDirection::Response);
    assert_eq!(exact.fields().len(), 2);
    assert_eq!(exact.clone(), exact);
}

#[test]
fn evidence_constructor_accepts_empty_and_rejects_noncanonical_identities() {
    let limits = schema_limits(2);
    let candidate = candidate(11, 7, 2);
    let requirement =
        SchemaRequirement::new(SchemaDirection::Request, vec![field(1, b"one")], limits)
            .expect("requirement");
    let ledger = ledger(vec![(
        1,
        b"Requests require one field.",
        ObligationSpec::RequestSchema(requirement),
        Vec::new(),
    )]);
    let evidence_binding = binding(&ledger, candidate, 1, Vec::new(), 41);

    let empty =
        SchemaEvidence::new(evidence_binding.clone(), SchemaDirection::Request, Vec::new(), limits)
            .expect("empty observation is canonical");
    assert!(empty.observed_fields().is_empty());

    let oversized = SchemaEvidence::new(
        evidence_binding.clone(),
        SchemaDirection::Request,
        vec![
            SchemaFieldId::new(late_digest(3)),
            SchemaFieldId::new(late_digest(2)),
            SchemaFieldId::new(late_digest(2)),
        ],
        limits,
    )
    .expect_err("size limit precedes observation ordering");
    assert_eq!(oversized.kind(), ObligationErrorKind::InvalidSchema);

    assert_eq!(
        error_kind(SchemaEvidence::new(
            evidence_binding.clone(),
            SchemaDirection::Request,
            vec![SchemaFieldId::new(late_digest(1)), SchemaFieldId::new(late_digest(1)),],
            limits,
        )),
        ObligationErrorKind::DuplicateValue,
    );
    assert_eq!(
        error_kind(SchemaEvidence::new(
            evidence_binding,
            SchemaDirection::Request,
            vec![SchemaFieldId::new(late_digest(2)), SchemaFieldId::new(late_digest(1)),],
            limits,
        )),
        ObligationErrorKind::NonCanonicalOrder,
    );
}

#[test]
fn covers_is_exact_directional_required_field_membership() {
    let limits = schema_limits(5);
    let requirement = SchemaRequirement::new(
        SchemaDirection::Request,
        vec![field(2, b"two"), field(4, b"four")],
        limits,
    )
    .expect("requirement");
    let candidate = candidate(11, 7, 2);
    let ledger = ledger(vec![(
        1,
        b"Requests require fields two and four.",
        ObligationSpec::RequestSchema(requirement.clone()),
        Vec::new(),
    )]);
    let evidence_binding = binding(&ledger, candidate, 1, Vec::new(), 51);
    let observed = |direction, identities: &[u8]| {
        SchemaEvidence::new(
            evidence_binding.clone(),
            direction,
            identities.iter().map(|last| SchemaFieldId::new(late_digest(*last))).collect(),
            limits,
        )
        .expect("canonical observation")
    };

    let superset = observed(SchemaDirection::Request, &[1, 2, 3, 4, 5]);
    assert!(superset.covers(&requirement));
    assert_eq!(superset.clone(), superset);
    let wrapped = RequirementEvidence::Schema(superset);
    let wrapped_clone = wrapped.clone();
    assert_eq!(wrapped_clone, wrapped);
    assert!(!observed(SchemaDirection::Request, &[1, 2, 3, 5]).covers(&requirement));
    assert!(!observed(SchemaDirection::Request, &[1, 2, 3]).covers(&requirement));
    assert!(!observed(SchemaDirection::Response, &[1, 2, 3, 4, 5]).covers(&requirement));
}

fn shape_mismatch(specification: ObligationSpec) -> ObligationError {
    let limits = ObligationLimits::production();
    let source = PublicTaskSource::new(b"directional schema".to_vec(), 7, limits).expect("source");
    RequirementLedger::extract(
        &source,
        vec![RequirementDraft::new(
            requirement_id(1),
            0,
            b"directional schema".len(),
            specification,
            Vec::new(),
        )],
        limits,
    )
    .expect_err("requirement direction mismatch")
}

#[test]
fn ledger_rejects_schema_variant_direction_mismatch() {
    let limits = ObligationLimits::production();
    let response =
        SchemaRequirement::new(SchemaDirection::Response, vec![field(1, b"response")], limits)
            .expect("response requirement");
    assert_eq!(
        shape_mismatch(ObligationSpec::RequestSchema(response)).kind(),
        ObligationErrorKind::RequirementShapeMismatch,
    );

    let request =
        SchemaRequirement::new(SchemaDirection::Request, vec![field(1, b"request")], limits)
            .expect("request requirement");
    assert_eq!(
        shape_mismatch(ObligationSpec::ResponseSchema(request)).kind(),
        ObligationErrorKind::RequirementShapeMismatch,
    );
}
