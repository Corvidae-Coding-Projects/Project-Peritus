//! Constructor, bounds, canonical-order, and scope behavior matrix.

mod support;

use peritus_memory::{
    BasisPoints, ClaimType, ClaimTypeSet, Confidence, EvidenceSet, FeatureKey, FeatureWeight,
    Feedback, MemoryErrorKind, MemoryEvidence, MemoryField, MemoryId, MemoryMaterial, MemoryScope,
    MemoryTiming, Observation, RepositoryId, RetrievalFeature, RetrievalFeatures, ScopeKind,
    ScopePolicy, SourceEventSet, SourceProvenance,
};
use peritus_types::{ActorId, EventId, EvidenceId, ProjectId, Sha256Digest};
use support::{event, evidence, feature_key, observation, project_scope, repository_scope};

#[test]
fn stable_identifiers_reject_zero_and_preserve_bytes() {
    assert_eq!(MemoryId::new([0; 16]).unwrap_err().kind(), MemoryErrorKind::ZeroIdentifier);
    assert_eq!(RepositoryId::new([0; 16]).unwrap_err().field_value(), MemoryField::RepositoryId);
    assert_eq!(FeatureKey::new([0; 16]).unwrap_err().field_value(), MemoryField::FeatureKey);
    assert_eq!(MemoryId::new([7; 16]).unwrap().into_bytes(), [7; 16]);
}

#[test]
fn bounded_scores_reject_invalid_values() {
    assert_eq!(BasisPoints::new(10_001).unwrap_err().kind(), MemoryErrorKind::InvalidBound);
    assert_eq!(Confidence::new(10_001).unwrap_err().kind(), MemoryErrorKind::InvalidBound);
    assert_eq!(FeatureWeight::new(0).unwrap_err().kind(), MemoryErrorKind::InvalidBound);
    assert_eq!(Feedback::new(10_001, 0).unwrap_err().kind(), MemoryErrorKind::InvalidBound);
}

#[test]
fn source_events_are_nonempty_bounded_and_canonical() {
    assert_eq!(SourceEventSet::new(Vec::new()).unwrap_err().kind(), MemoryErrorKind::EmptyValue);
    assert_eq!(
        SourceEventSet::new(vec![event(2), event(2)]).unwrap_err().kind(),
        MemoryErrorKind::DuplicateValue
    );
    assert_eq!(
        SourceEventSet::new(vec![event(2), event(1)]).unwrap_err().kind(),
        MemoryErrorKind::NonCanonicalOrder
    );
    let too_many = (0..257)
        .map(|index| {
            let mut bytes = [0_u8; 16];
            bytes[..2].copy_from_slice(&(index + 1_u16).to_be_bytes());
            EventId::new(bytes).unwrap()
        })
        .collect();
    assert_eq!(SourceEventSet::new(too_many).unwrap_err().kind(), MemoryErrorKind::LimitExceeded);
}

#[test]
fn evidence_sets_are_canonical_and_cannot_conflict() {
    assert_eq!(
        EvidenceSet::new(vec![evidence(3), evidence(2)]).unwrap_err().kind(),
        MemoryErrorKind::NonCanonicalOrder
    );
    assert_eq!(
        EvidenceSet::new(vec![evidence(2), evidence(2)]).unwrap_err().kind(),
        MemoryErrorKind::DuplicateValue
    );
    let error = MemoryEvidence::new(
        SourceEventSet::new(vec![event(1)]).unwrap(),
        EvidenceSet::new(vec![evidence(2)]).unwrap(),
        EvidenceSet::new(vec![evidence(2)]).unwrap(),
    )
    .unwrap_err();
    assert_eq!(error.kind(), MemoryErrorKind::ConflictingEvidence);
}

#[test]
fn material_checks_content_digest_and_token_bounds() {
    let bytes = b"quoted repository instruction".to_vec();
    assert_eq!(
        MemoryMaterial::new(
            ClaimType::Fact,
            Sha256Digest::new([9; 32]),
            bytes.clone(),
            SourceProvenance::Repository,
            3,
        )
        .unwrap_err()
        .kind(),
        MemoryErrorKind::DigestMismatch
    );
    let digest = peritus_codec::sha256(&bytes);
    assert_eq!(
        MemoryMaterial::new(ClaimType::Fact, digest, Vec::new(), SourceProvenance::Repository, 3,)
            .unwrap_err()
            .kind(),
        MemoryErrorKind::EmptyValue
    );
    assert_eq!(
        MemoryMaterial::new(ClaimType::Fact, digest, bytes, SourceProvenance::Repository, 0,)
            .unwrap_err()
            .kind(),
        MemoryErrorKind::InvalidBound
    );
}

#[test]
fn feature_and_claim_sets_require_canonical_order() {
    let one = RetrievalFeature::new(
        feature_key(1),
        Sha256Digest::new([1; 32]),
        FeatureWeight::new(1).unwrap(),
    );
    let two = RetrievalFeature::new(
        feature_key(2),
        Sha256Digest::new([2; 32]),
        FeatureWeight::new(1).unwrap(),
    );
    assert_eq!(
        RetrievalFeatures::new(vec![two, one]).unwrap_err().kind(),
        MemoryErrorKind::NonCanonicalOrder
    );
    assert_eq!(
        RetrievalFeatures::new(vec![one, one]).unwrap_err().kind(),
        MemoryErrorKind::DuplicateValue
    );
    assert_eq!(ClaimTypeSet::new(Vec::new()).unwrap_err().kind(), MemoryErrorKind::EmptyValue);
    assert_eq!(
        ClaimTypeSet::new(vec![ClaimType::Warning, ClaimType::Fact]).unwrap_err().kind(),
        MemoryErrorKind::NonCanonicalOrder
    );
}

#[test]
fn scopes_require_durable_and_kind_specific_dimensions() {
    let empty = MemoryScope::new(ScopeKind::Actor, None, None, None, None, None).unwrap_err();
    assert_eq!(empty.kind(), MemoryErrorKind::EmptyValue);
    let missing_actor = MemoryScope::new(
        ScopeKind::Actor,
        Some(ProjectId::new([1; 16]).unwrap()),
        None,
        None,
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(missing_actor.kind(), MemoryErrorKind::IncompleteScope);
    let actor = ActorId::new([8; 16]).unwrap();
    assert!(
        MemoryScope::new(
            ScopeKind::Actor,
            Some(ProjectId::new([1; 16]).unwrap()),
            None,
            None,
            Some(actor),
            None,
        )
        .is_ok()
    );
}

#[test]
fn scope_compatibility_is_exact_or_explicitly_broader() {
    let project = project_scope(1);
    let repository = repository_scope(1);
    assert!(!project.compatible_with(&repository, ScopePolicy::Exact));
    assert!(project.compatible_with(&repository, ScopePolicy::IncludeBroader));
    assert!(!repository.compatible_with(&project, ScopePolicy::IncludeBroader));
}

#[test]
fn timing_rejects_review_or_expiry_before_creation() {
    assert_eq!(
        MemoryTiming::new(observation(5), Some(observation(4)), None).unwrap_err().kind(),
        MemoryErrorKind::StaleObservation
    );
    assert_eq!(
        MemoryTiming::new(observation(5), None, Some(observation(4))).unwrap_err().kind(),
        MemoryErrorKind::ExpiryBeforeCreation
    );
    assert_eq!(Observation::new(0, 1).unwrap_err().kind(), MemoryErrorKind::InvalidBound);
}

#[test]
fn foundation_identifiers_still_reject_zero_before_memory_construction() {
    assert!(EventId::new([0; 16]).is_err());
    assert!(EvidenceId::new([0; 16]).is_err());
}
