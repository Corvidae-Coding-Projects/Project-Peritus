//! Public boundary regressions for exact obligation values and validation precedence.

mod support;

use peritus_obligations::{
    AlternativeBranchId, AlternativeGroupId, ConditionObservation, ConditionState, ObligationError,
    ObligationErrorKind, ObligationLimits, ObligationSpec, PathId, PathMention, PathRole,
    PublicTaskSource, RequirementDraft, RequirementLedger, SchemaDirection, SchemaField,
    SchemaFieldId, SchemaRequirement,
};
use peritus_types::Sha256Digest;
use support::{digest, requirement_id};

fn path(last: u8, role: PathRole) -> PathMention {
    let mut bytes = [0; 32];
    bytes[31] = last;
    PathMention::new(PathId::new(Sha256Digest::new(bytes)), vec![0, last, 0xff], role, 3)
        .expect("bounded binary path")
}

fn extract(
    paths: Vec<PathMention>,
    limits: ObligationLimits,
) -> Result<RequirementLedger, ObligationError> {
    let source = PublicTaskSource::new(b"exact clause".to_vec(), 0, limits).expect("source");
    RequirementLedger::extract(
        &source,
        vec![RequirementDraft::new(requirement_id(1), 0, 12, ObligationSpec::Hard, paths)],
        limits,
    )
}

fn assert_error(
    error: ObligationError,
    kind: ObligationErrorKind,
    requirement_id: Option<peritus_spec::RequirementId>,
    expected: Option<u64>,
    actual: Option<u64>,
) {
    assert_eq!(error.kind(), kind);
    assert_eq!(error.requirement_id(), requirement_id);
    assert_eq!(error.expected(), expected);
    assert_eq!(error.actual(), actual);
}

const fn draft(
    identity: u8,
    byte_start: usize,
    byte_end: usize,
    specification: ObligationSpec,
    paths: Vec<PathMention>,
) -> RequirementDraft {
    RequirementDraft::new(requirement_id(identity), byte_start, byte_end, specification, paths)
}

#[test]
fn public_extraction_retains_collection_and_interleaved_draft_error_priority() {
    let limits = ObligationLimits::new(8, 8, 1, 3, 2, 2).expect("focused limits");
    let source = PublicTaskSource::new(b"abcdefgh".to_vec(), 1, limits).expect("source");

    let empty = RequirementLedger::extract(&source, Vec::new(), limits)
        .expect_err("an empty draft collection is rejected before traversal");
    assert_error(empty, ObligationErrorKind::LimitExceeded, None, Some(1), Some(0));

    let oversized = RequirementLedger::extract(
        &source,
        vec![
            draft(2, 4, 4, ObligationSpec::Hard, Vec::new()),
            draft(1, 7, 6, ObligationSpec::Hard, Vec::new()),
        ],
        limits,
    )
    .expect_err("collection size precedes ordering and span validation");
    assert_error(oversized, ObligationErrorKind::LimitExceeded, None, Some(1), Some(2));

    let traversal_limits = ObligationLimits::new(8, 8, 3, 3, 2, 2).expect("traversal limits");
    let invalid_first = RequirementLedger::extract(
        &source,
        vec![
            draft(1, 4, 4, ObligationSpec::Hard, Vec::new()),
            draft(1, 0, 1, ObligationSpec::Hard, Vec::new()),
        ],
        traversal_limits,
    )
    .expect_err("the first draft fails before a later duplicate pair is inspected");
    assert_error(
        invalid_first,
        ObligationErrorKind::InvalidClauseSpan,
        Some(requirement_id(1)),
        None,
        None,
    );

    let duplicate = RequirementLedger::extract(
        &source,
        vec![
            draft(2, 0, 1, ObligationSpec::Hard, Vec::new()),
            draft(2, 4, 4, ObligationSpec::Hard, Vec::new()),
        ],
        traversal_limits,
    )
    .expect_err("a duplicate pair precedes the current draft span");
    assert_error(
        duplicate,
        ObligationErrorKind::DuplicateValue,
        Some(requirement_id(2)),
        None,
        None,
    );

    let descending = RequirementLedger::extract(
        &source,
        vec![
            draft(2, 0, 1, ObligationSpec::Hard, Vec::new()),
            draft(1, 4, 4, ObligationSpec::Hard, Vec::new()),
        ],
        traversal_limits,
    )
    .expect_err("a descending pair precedes the current draft span");
    assert_error(
        descending,
        ObligationErrorKind::NonCanonicalOrder,
        Some(requirement_id(1)),
        None,
        None,
    );
}

#[test]
fn public_extraction_retains_span_shape_path_and_alternative_error_priority() {
    let limits = ObligationLimits::new(8, 8, 3, 2, 2, 2).expect("focused limits");
    let source = PublicTaskSource::new(b"abcdefgh".to_vec(), 1, limits).expect("source");
    let response_field = SchemaField::new(
        SchemaFieldId::new(digest(1)),
        b"field".to_vec(),
        limits.max_clause_bytes(),
    )
    .expect("field");
    let response = SchemaRequirement::new(SchemaDirection::Response, vec![response_field], limits)
        .expect("response schema");
    let wrong_shape = ObligationSpec::RequestSchema(response);
    let noncanonical_paths = vec![
        path(3, PathRole::RequiredOutput),
        path(2, PathRole::RequiredOutput),
        path(1, PathRole::RequiredOutput),
    ];

    let span = RequirementLedger::extract(
        &source,
        vec![draft(1, 3, 3, wrong_shape.clone(), noncanonical_paths.clone())],
        limits,
    )
    .expect_err("span validation precedes requirement shape and paths");
    assert_error(span, ObligationErrorKind::InvalidClauseSpan, Some(requirement_id(1)), None, None);

    let shape = RequirementLedger::extract(
        &source,
        vec![draft(1, 0, 1, wrong_shape, noncanonical_paths.clone())],
        limits,
    )
    .expect_err("shape validation precedes path validation");
    assert_error(shape, ObligationErrorKind::RequirementShapeMismatch, None, None, None);

    let paths = RequirementLedger::extract(
        &source,
        vec![draft(1, 0, 1, ObligationSpec::Hard, noncanonical_paths)],
        limits,
    )
    .expect_err("path size precedes path ordering");
    assert_error(paths, ObligationErrorKind::LimitExceeded, None, Some(2), Some(3));

    let invalid_alternative = ObligationSpec::Alternative {
        group_id: AlternativeGroupId::new(digest(4)),
        branch_id: AlternativeBranchId::new(digest(5)),
    };
    let alternative = RequirementLedger::extract(
        &source,
        vec![draft(1, 0, 1, invalid_alternative, Vec::new())],
        limits,
    )
    .expect_err("alternative topology is checked after every draft");
    assert_error(alternative, ObligationErrorKind::InvalidAlternative, None, None, None);
}

#[test]
fn path_validation_retains_size_and_first_pair_error_precedence() {
    let limits = ObligationLimits::new(64, 32, 8, 3, 2, 4).expect("limits");
    let paths = |ids: &[u8]| ids.iter().map(|id| path(*id, PathRole::RequiredOutput)).collect();
    let duplicate = extract(paths(&[2, 2, 1]), limits).expect_err("first pair duplicates");
    assert_eq!(duplicate.kind(), ObligationErrorKind::DuplicateValue);
    assert_eq!(
        (duplicate.requirement_id(), duplicate.expected(), duplicate.actual()),
        (None, None, None)
    );
    let descending = extract(paths(&[2, 1, 1]), limits).expect_err("first pair descends");
    assert_eq!(descending.kind(), ObligationErrorKind::NonCanonicalOrder);
    let oversized = extract(paths(&[2, 1, 1, 1]), limits).expect_err("size precedes order");
    assert_eq!(oversized.kind(), ObligationErrorKind::LimitExceeded);
    assert_eq!(
        (oversized.requirement_id(), oversized.expected(), oversized.actual()),
        (None, Some(3), Some(4))
    );
    assert!(extract(paths(&[1, 2, 3]), limits).is_ok());
    assert!(extract(Vec::new(), limits).is_ok());
}

#[test]
fn ledger_clone_preserves_exact_binary_clause_provenance_and_each_path_role() {
    let limits = ObligationLimits::production();
    let source = PublicTaskSource::new(vec![b'a', 0, 0xff, b'z'], 0, limits).expect("source");
    let source_copy = source.clone();
    assert_eq!(source_copy, source);
    let roles = [
        PathRole::RequiredOutput,
        PathRole::RequiredModification,
        PathRole::RequiredInput,
        PathRole::Reference,
        PathRole::Example,
    ];
    let paths: Vec<_> = roles.into_iter().zip(1..=5).map(|(role, id)| path(id, role)).collect();
    for (index, item) in paths.iter().enumerate() {
        assert_eq!(item.role().requires_candidate_evidence(), index < 2);
        assert_eq!(item.clone(), *item);
    }
    let ledger = RequirementLedger::extract(
        &source,
        vec![RequirementDraft::new(
            requirement_id(7),
            1,
            3,
            ObligationSpec::GeneratedOutput,
            paths,
        )],
        limits,
    )
    .expect("exact clause extraction");
    let copy = ledger.clone();
    assert_eq!(copy, ledger);
    let entry = copy.entry(requirement_id(7)).expect("cloned entry");
    assert_eq!(entry.clause().exact(), &[0, 0xff]);
    let provenance = entry.clause().provenance();
    assert_eq!(provenance.source_digest(), source.digest());
    assert_eq!(provenance.conversation_revision(), 0);
    assert_eq!(provenance.ordinal(), 0);
    assert_eq!((provenance.byte_start(), provenance.byte_end()), (1, 3));
    assert_eq!(entry.paths().len(), 5);
    for (actual, expected) in entry.paths().iter().zip(roles) {
        assert_eq!(actual.role(), expected);
        assert_eq!(actual.exact().first(), Some(&0));
        assert_eq!(actual.exact().last(), Some(&0xff));
    }
}

#[test]
fn condition_and_requirement_identity_errors_preserve_complete_details() {
    let limits = ObligationLimits::production();
    let observation = ConditionObservation::new(
        peritus_obligations::ConditionId::new(digest(12)),
        ConditionState::Unknown,
        digest(13),
    );
    assert_eq!(observation.condition_id().digest(), digest(12));
    assert_eq!(observation.state(), ConditionState::Unknown);
    assert_eq!(observation.observation_digest(), digest(13));
    let source = PublicTaskSource::new(b"ab".to_vec(), 5, limits).expect("source");
    let error = RequirementLedger::extract(
        &source,
        vec![
            RequirementDraft::new(requirement_id(9), 0, 1, ObligationSpec::Hard, Vec::new()),
            RequirementDraft::new(requirement_id(9), 1, 2, ObligationSpec::Hard, Vec::new()),
        ],
        limits,
    )
    .expect_err("duplicate requirement identity");
    assert_eq!(error.kind(), ObligationErrorKind::DuplicateValue);
    assert_eq!(error.requirement_id(), Some(requirement_id(9)));
    assert_eq!((error.expected(), error.actual()), (None, None));
}

#[test]
fn limits_and_public_source_preserve_exact_admission_and_error_details() {
    for zero_index in 0..6 {
        let mut bounds = [4; 6];
        bounds[zero_index] = 0;
        let error =
            ObligationLimits::new(bounds[0], bounds[1], bounds[2], bounds[3], bounds[4], bounds[5])
                .expect_err("each bound must be nonzero");
        assert_eq!(error.kind(), ObligationErrorKind::InvalidLimit);
        assert_eq!((error.requirement_id(), error.expected(), error.actual()), (None, None, None));
    }
    assert!(ObligationLimits::new(1, 2, 1, 1, 1, 1).is_err());
    let limits = ObligationLimits::new(3, 3, 1, 1, 1, 1).expect("exact bound");
    for bytes in [Vec::new(), vec![0; 4]] {
        let actual = bytes.len() as u64;
        let error = PublicTaskSource::new(bytes, 0, limits).expect_err("source size rejected");
        assert_eq!(error.kind(), ObligationErrorKind::InvalidSource);
        assert_eq!(
            (error.requirement_id(), error.expected(), error.actual()),
            (None, Some(3), Some(actual))
        );
    }
    assert_eq!(
        PublicTaskSource::new(vec![0; 3], 0, limits).expect("exact source bound").content(),
        &[0; 3]
    );
}
