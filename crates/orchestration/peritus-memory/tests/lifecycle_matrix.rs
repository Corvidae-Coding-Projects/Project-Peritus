//! Immutable memory lifecycle transition matrix.

mod support;

use peritus_memory::{
    Confidence, DeletionReason, EvidenceSet, Feedback, MemoryErrorKind, MemoryState,
    QuarantineReason, deletion_dominates, lifecycle_advanced,
};
use support::{RecordOptions, evidence, make_record, memory_id, observation, revision};

#[test]
fn review_returns_a_new_advanced_revision_without_mutating_original() {
    let original = make_record(RecordOptions::new(1));
    let reviewed = original
        .review(
            revision(2),
            observation(3),
            Confidence::new(9_000).unwrap(),
            EvidenceSet::new(vec![evidence(41), evidence(42)]).unwrap(),
            EvidenceSet::empty(),
            Feedback::new(2, 0).unwrap(),
        )
        .unwrap();
    assert_eq!(original.revision().get(), 1);
    assert_eq!(reviewed.revision().get(), 2);
    assert_eq!(reviewed.timing().reviewed(), Some(observation(3)));
    assert!(lifecycle_advanced(&original, &reviewed));
}

#[test]
fn stale_revisions_and_observations_are_rejected() {
    let record = make_record(RecordOptions::new(2));
    assert_eq!(
        record
            .quarantine(revision(1), observation(3), QuarantineReason::ManualReview)
            .unwrap_err()
            .kind(),
        MemoryErrorKind::InvalidRevision
    );
    assert_eq!(
        record
            .quarantine(revision(2), observation(2), QuarantineReason::ManualReview)
            .unwrap_err()
            .kind(),
        MemoryErrorKind::StaleObservation
    );
}

#[test]
fn quarantine_release_requires_a_later_review_and_revision() {
    let active = make_record(RecordOptions::new(3));
    let quarantined = active
        .quarantine(revision(2), observation(3), QuarantineReason::SuspectedPoisoning)
        .unwrap();
    assert_eq!(quarantined.lifecycle().state(), MemoryState::Quarantined);
    assert_eq!(
        quarantined
            .release(
                revision(3),
                observation(3),
                Confidence::new(7_000).unwrap(),
                Feedback::none(),
            )
            .unwrap_err()
            .kind(),
        MemoryErrorKind::ReleaseRequiresReview
    );
    let released = quarantined
        .release(revision(3), observation(4), Confidence::new(7_000).unwrap(), Feedback::none())
        .unwrap();
    assert_eq!(released.lifecycle().state(), MemoryState::Active);
    assert_eq!(released.timing().reviewed(), Some(observation(4)));
}

#[test]
fn review_does_not_implicitly_release_quarantine() {
    let quarantined = make_record(RecordOptions::new(4))
        .quarantine(revision(2), observation(3), QuarantineReason::Contradiction)
        .unwrap();
    let reviewed = quarantined
        .review(
            revision(3),
            observation(4),
            Confidence::new(6_000).unwrap(),
            EvidenceSet::new(vec![evidence(44)]).unwrap(),
            EvidenceSet::empty(),
            Feedback::none(),
        )
        .unwrap();
    assert_eq!(reviewed.lifecycle().state(), MemoryState::Quarantined);
}

#[test]
fn active_may_expire_or_be_superseded_but_states_do_not_reopen() {
    let active = make_record(RecordOptions::new(5));
    let expired = active.expire(revision(2), observation(3)).unwrap();
    assert_eq!(expired.lifecycle().state(), MemoryState::Expired);
    assert_eq!(
        expired.expire(revision(3), observation(4)).unwrap_err().kind(),
        MemoryErrorKind::InvalidTransition
    );
    let superseded = active.supersede(revision(2), observation(3), memory_id(6)).unwrap();
    assert_eq!(superseded.lifecycle().state(), MemoryState::Superseded);
    assert_eq!(superseded.lifecycle().superseded_by(), Some(memory_id(6)));
    assert_eq!(
        active.supersede(revision(2), observation(3), active.id()).unwrap_err().kind(),
        MemoryErrorKind::DuplicateValue
    );
}

#[test]
fn forgetting_produces_content_free_dominant_tombstone() {
    let record = make_record(RecordOptions::new(7));
    let tombstone =
        record.forget(revision(2), observation(3), DeletionReason::UserRequest).unwrap();
    assert_eq!(tombstone.memory_id(), record.id());
    assert_eq!(tombstone.last_known_revision().get(), 2);
    assert_eq!(tombstone.prior_digest(), record.content_digest());
    assert!(tombstone.dominates(&record));
    assert!(deletion_dominates(&tombstone, &record));
}

#[test]
fn forgetting_is_available_from_every_retained_state() {
    let active = make_record(RecordOptions::new(8));
    let quarantined =
        active.quarantine(revision(2), observation(3), QuarantineReason::ManualReview).unwrap();
    let expired = active.expire(revision(2), observation(3)).unwrap();
    let superseded = active.supersede(revision(2), observation(3), memory_id(9)).unwrap();
    for record in [&active, &quarantined, &expired, &superseded] {
        assert!(
            record.forget(revision(4), observation(5), DeletionReason::RetentionPolicy).is_ok()
        );
    }
}
