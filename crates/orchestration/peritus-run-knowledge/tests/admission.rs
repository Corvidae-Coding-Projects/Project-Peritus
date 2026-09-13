//! Public admission and error precedence across the production knowledge constructors.

mod support;

use peritus_role::HarnessRole;
use peritus_run_knowledge::{
    CurrentKnowledgeState, InvalidationRequest, KnowledgeBinding, KnowledgeChange, KnowledgeError,
    KnowledgeErrorKind, KnowledgeLimits, KnowledgeSection, KnowledgeSectionKind, KnowledgeSourceId,
    SourceDigest,
};
use support::{candidate, digest, limits, section_id, source_id};

fn assert_plain(error: KnowledgeError, kind: KnowledgeErrorKind) {
    assert_eq!(error.kind(), kind);
    assert_eq!(error.section_id(), None);
    assert_eq!(error.source_id(), None);
    assert_eq!(error.expected(), None);
    assert_eq!(error.actual(), None);
}

fn bound_sources(ids: &[u8]) -> Vec<SourceDigest> {
    ids.iter().map(|id| SourceDigest::new(source_id(*id), digest(*id))).collect()
}

#[test]
fn every_limit_is_required_and_exact_maximum_values_are_retained() {
    for zero in 0..4 {
        let mut values = [usize::MAX; 4];
        values[zero] = 0;
        assert_plain(
            KnowledgeLimits::new(values[0], values[1], values[2], values[3])
                .expect_err("each bound must be nonzero"),
            KnowledgeErrorKind::InvalidLimit,
        );
    }
    let value = KnowledgeLimits::new(usize::MAX, 2, 3, 4).expect("nonzero limits");
    assert_eq!(value.max_sections(), usize::MAX);
    assert_eq!(value.max_catalog_sources(), 2);
    assert_eq!(value.max_sources_per_section(), 3);
    assert_eq!(value.max_dependencies_per_section(), 4);
}

#[test]
fn binding_preserves_role_sequence_source_error_precedence_and_future_sequence_admission() {
    let identity = candidate(20, 1, 3);
    for role in [HarnessRole::Evaluator, HarnessRole::Evolver] {
        assert_plain(
            KnowledgeBinding::new(identity, role, 0, Vec::new(), limits())
                .expect_err("role rejection precedes other invalid inputs"),
            KnowledgeErrorKind::UnsupportedRole,
        );
    }
    for role in [HarnessRole::Writer, HarnessRole::Reviewer, HarnessRole::Fixer] {
        assert_plain(
            KnowledgeBinding::new(identity, role, 0, Vec::new(), limits())
                .expect_err("zero sequence precedes empty sources"),
            KnowledgeErrorKind::ZeroCreationSequence,
        );
        assert_plain(
            KnowledgeBinding::new(identity, role, 1, Vec::new(), limits())
                .expect_err("sources required"),
            KnowledgeErrorKind::EmptyCollection,
        );
        let value = KnowledgeBinding::new(identity, role, u64::MAX, bound_sources(&[1]), limits())
            .expect("the binding constructor does not compare creation to candidate checkpoint");
        assert_eq!(value.creation_sequence(), u64::MAX);
        assert_eq!(*value.candidate(), identity);
        assert_eq!(value.role(), role);
        assert_eq!(value.clone(), value);
    }
}

#[test]
fn source_catalog_errors_retain_first_offending_identity_and_size_precedence() {
    let identity = candidate(20, 1, 3);
    let small = KnowledgeLimits::new(8, 2, 2, 2).expect("small bounds");
    for binding in [false, true] {
        let make = |ids: &[u8]| {
            if binding {
                KnowledgeBinding::new(identity, HarnessRole::Writer, 1, bound_sources(ids), small)
                    .map(|_| ())
            } else {
                CurrentKnowledgeState::new(identity, bound_sources(ids), small).map(|_| ())
            }
        };
        let size = make(&[2, 1, 1]).expect_err("size must precede order and duplicate errors");
        assert_eq!(size.kind(), KnowledgeErrorKind::LimitExceeded);
        assert_eq!(size.expected(), Some(2));
        assert_eq!(size.actual(), Some(3));
        assert_eq!(size.source_id(), None);
        assert_eq!(size.section_id(), None);
        for (ids, kind) in [
            ([1, 1], KnowledgeErrorKind::DuplicateValue),
            ([2, 1], KnowledgeErrorKind::NonCanonicalOrder),
        ] {
            let error = make(&ids).expect_err("noncanonical sources");
            assert_eq!(error.kind(), kind);
            assert_eq!(error.source_id(), Some(source_id(1)));
            assert_eq!(error.section_id(), None);
            assert_eq!(error.expected(), None);
            assert_eq!(error.actual(), None);
        }
    }
    let error = CurrentKnowledgeState::new(identity, bound_sources(&[2, 1, 1]), limits())
        .expect_err("first descending pair precedes later duplicate");
    assert_eq!(error.kind(), KnowledgeErrorKind::NonCanonicalOrder);
    assert_eq!(error.source_id(), Some(source_id(1)));
}

#[test]
fn source_order_uses_all_sixteen_bytes_and_matches_derived_order() {
    let identity = candidate(20, 1, 3);
    for index in 0..16 {
        let mut lower = [1; 16];
        let mut upper = lower;
        lower[index] = 2;
        upper[index] = 3;
        let lower = KnowledgeSourceId::new(lower).expect("lower");
        let upper = KnowledgeSourceId::new(upper).expect("upper");
        assert!(lower < upper);
        let first = SourceDigest::new(lower, digest(1));
        let last = SourceDigest::new(upper, digest(2));
        let value = CurrentKnowledgeState::new(identity, vec![first, last], limits())
            .expect("increasing complete identities");
        assert_eq!(value.sources(), &[first, last]);
        assert_eq!(value.clone(), value);
        let error = CurrentKnowledgeState::new(identity, vec![last, first], limits())
            .expect_err("decreasing at any identity byte");
        assert_eq!(error.kind(), KnowledgeErrorKind::NonCanonicalOrder);
        assert_eq!(error.source_id(), Some(lower));
    }
}

#[test]
fn section_and_request_validation_preserve_shape_size_and_member_error_precedence() {
    let identity = candidate(20, 1, 3);
    let binding =
        KnowledgeBinding::new(identity, HarnessRole::Writer, 1, bound_sources(&[1]), limits())
            .expect("binding");
    let small = KnowledgeLimits::new(8, 8, 8, 1).expect("small dependency bound");
    let make = |ids: &[u8], bounds| {
        KnowledgeSection::new(
            section_id(2),
            KnowledgeSectionKind::DesignSection,
            digest(11),
            binding.clone(),
            ids.iter().map(|id| section_id(*id)).collect(),
            bounds,
        )
    };
    let size = make(&[2, 2], small).expect_err("size precedes self-reference");
    assert_eq!(size.kind(), KnowledgeErrorKind::LimitExceeded);
    assert_eq!(size.expected(), Some(1));
    assert_eq!(size.actual(), Some(2));
    for (ids, kind, id) in [
        (vec![3, 2], KnowledgeErrorKind::SelfDependency, 2),
        (vec![3, 1, 2], KnowledgeErrorKind::NonCanonicalOrder, 1),
        (vec![1, 1, 2], KnowledgeErrorKind::DuplicateValue, 1),
    ] {
        let error = make(&ids, limits()).expect_err("first invalid member");
        assert_eq!(error.kind(), kind);
        assert_eq!(error.section_id(), Some(section_id(id)));
        assert_eq!(error.source_id(), None);
        assert_eq!(error.expected(), None);
        assert_eq!(error.actual(), None);
    }
    let state = CurrentKnowledgeState::new(identity, bound_sources(&[1]), limits()).expect("state");
    assert_plain(
        InvalidationRequest::new(state.clone(), KnowledgeChange::UserClarification, Vec::new())
            .expect_err("clarification requires targets"),
        KnowledgeErrorKind::EmptyCollection,
    );
    for change in [
        KnowledgeChange::SameRevision,
        KnowledgeChange::ConversationRevision,
        KnowledgeChange::SourceChanged,
        KnowledgeChange::CandidateRevision,
        KnowledgeChange::ProviderFailure,
    ] {
        assert_plain(
            InvalidationRequest::new(state.clone(), change, vec![section_id(2), section_id(1)])
                .expect_err("change shape precedes noncanonical targets"),
            KnowledgeErrorKind::InvalidChangeRequest,
        );
        InvalidationRequest::new(state.clone(), change, Vec::new()).expect("no targets allowed");
    }
    let error = InvalidationRequest::new(
        state,
        KnowledgeChange::UserClarification,
        vec![section_id(2), section_id(1), section_id(1)],
    )
    .expect_err("first descending pair precedes later duplicate");
    assert_eq!(error.kind(), KnowledgeErrorKind::NonCanonicalOrder);
    assert_eq!(error.section_id(), Some(section_id(1)));
}
