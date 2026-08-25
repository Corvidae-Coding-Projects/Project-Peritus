//! Canonical index replay, tombstone, posting, and digest behavior matrix.

mod support;

use peritus_memory::{
    BasisPoints, ClaimType, ClaimTypeSet, Confidence, DeletionReason, EvidenceSet, Feedback,
    FeedbackPolicy, MemoryErrorKind, MemoryIndex, MemoryTombstone, RankingWeights,
    RequiredFeatures, RetrievalFeatures, RetrievalLimits, RetrievalPolicy, RetrievalQuery,
    ScopePolicy,
};
use peritus_role::{HarnessRole, RoleProfile};
use peritus_types::Sha256Digest;
use support::{
    RecordOptions, evidence, make_record, observation, project_scope, repository_scope, revision,
};

fn policy() -> RetrievalPolicy {
    RetrievalPolicy::new(
        RetrievalLimits::new(20, Confidence::new(0).unwrap(), None).unwrap(),
        ClaimTypeSet::new(vec![ClaimType::Fact]).unwrap(),
        RankingWeights::new(
            BasisPoints::new(2_000).unwrap(),
            BasisPoints::new(2_000).unwrap(),
            BasisPoints::new(2_000).unwrap(),
            BasisPoints::new(2_000).unwrap(),
            BasisPoints::new(1_000).unwrap(),
            BasisPoints::new(1_000).unwrap(),
        )
        .unwrap(),
        FeedbackPolicy::new(None, None),
        ScopePolicy::Exact,
    )
}

fn query() -> RetrievalQuery {
    RetrievalQuery::new(
        project_scope(1),
        RoleProfile::for_harness_role(HarnessRole::Writer),
        observation(10),
        RetrievalFeatures::empty(),
        RequiredFeatures::empty(),
        1_000,
    )
    .unwrap()
}

#[test]
fn rebuild_selects_latest_active_revision() {
    let original = make_record(RecordOptions::new(1));
    let reviewed = original
        .review(
            revision(2),
            observation(3),
            Confidence::new(9_000).unwrap(),
            EvidenceSet::new(vec![evidence(41)]).unwrap(),
            EvidenceSet::empty(),
            Feedback::new(2, 0).unwrap(),
        )
        .unwrap();
    let index = MemoryIndex::rebuild(vec![original, reviewed], Vec::new()).unwrap();
    assert_eq!(index.active_records().len(), 1);
    assert_eq!(index.active_records()[0].revision().get(), 2);
}

#[test]
fn latest_inactive_revision_is_absent_from_active_index() {
    let original = make_record(RecordOptions::new(2));
    let expired = original.expire(revision(2), observation(3)).unwrap();
    let index = MemoryIndex::rebuild(vec![original, expired], Vec::new()).unwrap();
    assert!(index.active_records().is_empty());
    assert!(index.scope_postings().is_empty());
}

#[test]
fn tombstone_dominates_records_at_or_below_its_revision() {
    let original = make_record(RecordOptions::new(3));
    let tombstone =
        original.forget(revision(2), observation(3), DeletionReason::UserRequest).unwrap();
    let index = MemoryIndex::rebuild(vec![original], vec![tombstone]).unwrap();
    assert!(index.active_records().is_empty());
    assert_eq!(index.tombstones().len(), 1);
}

#[test]
fn record_newer_than_tombstone_survives_replay() {
    let original = make_record(RecordOptions::new(4));
    let tombstone =
        original.forget(revision(2), observation(3), DeletionReason::RetentionPolicy).unwrap();
    let newer = original
        .review(
            revision(3),
            observation(4),
            Confidence::new(8_500).unwrap(),
            EvidenceSet::new(vec![evidence(44)]).unwrap(),
            EvidenceSet::empty(),
            Feedback::none(),
        )
        .unwrap();
    let index = MemoryIndex::rebuild(vec![original, newer], vec![tombstone]).unwrap();
    assert_eq!(index.active_records().len(), 1);
    assert_eq!(index.active_records()[0].revision().get(), 3);
}

#[test]
fn replay_rejects_noncanonical_and_duplicate_revisions() {
    let one = make_record(RecordOptions::new(5));
    let two = make_record(RecordOptions::new(6));
    assert_eq!(
        MemoryIndex::rebuild(vec![two, one.clone()], Vec::new()).unwrap_err().kind(),
        MemoryErrorKind::NonCanonicalOrder
    );
    assert_eq!(
        MemoryIndex::rebuild(vec![one.clone(), one], Vec::new()).unwrap_err().kind(),
        MemoryErrorKind::ConflictingRevision
    );
}

#[test]
fn replay_rejects_noncanonical_tombstones_and_digest_conflicts() {
    let one = make_record(RecordOptions::new(7));
    let two = make_record(RecordOptions::new(8));
    let one_tombstone =
        one.forget(revision(2), observation(3), DeletionReason::UserRequest).unwrap();
    let two_tombstone =
        two.forget(revision(2), observation(3), DeletionReason::UserRequest).unwrap();
    assert_eq!(
        MemoryIndex::rebuild(vec![one.clone(), two], vec![two_tombstone, one_tombstone],)
            .unwrap_err()
            .kind(),
        MemoryErrorKind::NonCanonicalOrder
    );
    let bad = MemoryTombstone::new(
        one.id(),
        revision(1),
        observation(3),
        DeletionReason::InvalidContent,
        Sha256Digest::new([99; 32]),
    );
    assert_eq!(
        MemoryIndex::rebuild(vec![one], vec![bad]).unwrap_err().kind(),
        MemoryErrorKind::TombstoneDigestMismatch
    );
}

#[test]
fn posting_lists_are_canonical_and_derived_only_from_active_records() {
    let one = make_record(RecordOptions::new(9));
    let mut two_options = RecordOptions::new(10);
    two_options.scope = repository_scope(2);
    let two = make_record(two_options);
    let index = MemoryIndex::rebuild(vec![one, two], Vec::new()).unwrap();
    assert_eq!(index.scope_postings().len(), 2);
    assert_eq!(index.claim_postings().len(), 1);
    assert_eq!(index.claim_postings()[0].memory_ids().len(), 2);
    assert_eq!(index.feature_postings().len(), 2);
    assert!(index.claim_postings()[0].memory_ids()[0] < index.claim_postings()[0].memory_ids()[1]);
}

#[test]
fn identical_canonical_rebuilds_have_identical_real_sha256_digests() {
    let one = make_record(RecordOptions::new(11));
    let two = make_record(RecordOptions::new(12));
    let first = MemoryIndex::rebuild(vec![one.clone(), two.clone()], Vec::new()).unwrap();
    let second = MemoryIndex::rebuild(vec![one, two], Vec::new()).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.digest(), second.digest());
    assert_ne!(first.digest().into_bytes(), [0; 32]);
}

#[test]
fn active_or_tombstone_changes_alter_index_digest() {
    let one = make_record(RecordOptions::new(13));
    let two = make_record(RecordOptions::new(14));
    let one_only = MemoryIndex::rebuild(vec![one.clone()], Vec::new()).unwrap();
    let both = MemoryIndex::rebuild(vec![one.clone(), two], Vec::new()).unwrap();
    assert_ne!(one_only.digest(), both.digest());
    let tombstone = one.forget(revision(2), observation(3), DeletionReason::UserRequest).unwrap();
    let deleted = MemoryIndex::rebuild(Vec::new(), vec![tombstone]).unwrap();
    let empty = MemoryIndex::rebuild(Vec::new(), Vec::new()).unwrap();
    assert_ne!(deleted.digest(), empty.digest());
}

#[test]
fn index_retrieval_matches_full_scan_of_canonical_active_view() {
    let one = make_record(RecordOptions::new(15));
    let two = make_record(RecordOptions::new(16));
    let index = MemoryIndex::rebuild(vec![one, two], Vec::new()).unwrap();
    let indexed = index.retrieve(&policy(), &query()).unwrap();
    let scanned =
        peritus_memory::retrieve(index.active_records(), &[], &policy(), &query()).unwrap();
    assert_eq!(indexed, scanned);
}
