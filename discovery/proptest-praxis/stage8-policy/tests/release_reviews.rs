use peritus_release_policy::{
    Architecture, CandidateId, EvidenceBinding, GitCommitId, OperatingSystem, PlatformIdentity,
    PlatformMatrix, PrincipalId, ProfileIdentity, ReleaseCandidate, ReleaseEvidence,
    ReleaseVersion, ReviewId, ReviewObservation, ReviewOutcome, SchemaIdentity, ToolchainIdentity,
    evaluate_release,
};
use peritus_types::Sha256Digest;
use proptest::prelude::*;

fn d(tag: u8) -> Sha256Digest {
    Sha256Digest::new([tag.max(1); 32])
}

fn candidate(id: u8) -> ReleaseCandidate {
    let platform = |os| PlatformIdentity::new(os, Architecture::X86_64, d(3)).unwrap();
    ReleaseCandidate::new(
        CandidateId::new([id; 16]).unwrap(),
        GitCommitId::sha1([2; 20]).unwrap(),
        ReleaseVersion::new(0, 1, 0, d(4)).unwrap(),
        PlatformMatrix::new(
            platform(OperatingSystem::Linux),
            platform(OperatingSystem::MacOs),
            platform(OperatingSystem::Windows),
        )
        .unwrap(),
        ToolchainIdentity::new(d(5), d(6), d(7), d(8)).unwrap(),
        ProfileIdentity::new(1, d(9)).unwrap(),
        SchemaIdentity::new(1, 1, 1, 1, d(10)).unwrap(),
        1,
        d(11),
    )
    .unwrap()
}

fn review(index: usize, kind: u8, reviewer: u8, context: u8) -> ReviewObservation {
    let binding = EvidenceBinding::new(
        candidate(if kind == 5 { 2 } else { 1 }),
        1,
        if kind == 4 { 9 } else { 20 },
        1,
        1,
    )
    .unwrap();
    ReviewObservation::new(
        ReviewId::new([u8::try_from(index + 1).unwrap(); 16]).unwrap(),
        binding,
        PrincipalId::new([if kind == 2 { 9 } else { reviewer }; 16]).unwrap(),
        PrincipalId::new([9; 16]).unwrap(),
        d(context),
        d(20),
        if kind == 1 { ReviewOutcome::ChangesRequired } else { ReviewOutcome::Approved },
        kind != 3,
    )
    .unwrap()
}

#[derive(Default, Debug)]
struct Expected {
    approved: u16,
    stale: u16,
    mismatched: u16,
    changes: u16,
    self_review: u16,
    non_independent: u16,
    duplicate_reviewer: bool,
    shared_context: bool,
}

fn model(rows: &[(u8, u8, u8)]) -> Expected {
    let mut expected = Expected::default();
    let mut seen_reviewers = Vec::new();
    let mut seen_contexts = Vec::new();
    for &(kind, reviewer, context) in rows {
        match kind {
            4 => {
                expected.stale += 1;
                continue;
            }
            5 => {
                expected.mismatched += 1;
                continue;
            }
            _ => {}
        }
        let actual_reviewer = if kind == 2 { 9 } else { reviewer };
        if kind == 2 {
            expected.self_review += 1;
        }
        if kind == 3 {
            expected.non_independent += 1;
        }
        if kind == 1 {
            expected.changes += 1;
        }
        if kind == 0 {
            expected.approved += 1;
        }
        if seen_reviewers.contains(&actual_reviewer) {
            expected.duplicate_reviewer = true;
        }
        if seen_contexts.contains(&context) {
            expected.shared_context = true;
        }
        seen_reviewers.push(actual_reviewer);
        seen_contexts.push(context);
    }
    expected
}

fn check(rows: &[(u8, u8, u8)]) {
    let reviews: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(i, &(kind, reviewer, context))| review(i, kind, reviewer, context))
        .collect();
    let decision = evaluate_release(
        candidate(1),
        10,
        &ReleaseEvidence::new(vec![], vec![], reviews.clone(), vec![], vec![]).unwrap(),
    );
    let actual = decision.reviews();
    let expected = model(rows);
    assert_eq!(actual.approved_count(), expected.approved, "rows={rows:?}");
    assert_eq!(actual.stale_count(), expected.stale, "rows={rows:?}");
    assert_eq!(actual.mismatched_count(), expected.mismatched, "rows={rows:?}");
    assert_eq!(actual.changes_required_count(), expected.changes, "rows={rows:?}");
    assert_eq!(actual.self_review_count(), expected.self_review, "rows={rows:?}");
    assert_eq!(actual.non_independent_count(), expected.non_independent, "rows={rows:?}");
    assert_eq!(actual.has_duplicate_reviewer(), expected.duplicate_reviewer, "rows={rows:?}");
    assert_eq!(actual.has_shared_context(), expected.shared_context, "rows={rows:?}");
    assert!(!actual.has_conflicting_review()); // Review IDs are unique in this family.
    let satisfied = expected.approved >= 2
        && expected.stale == 0
        && expected.mismatched == 0
        && expected.changes == 0
        && expected.self_review == 0
        && expected.non_independent == 0
        && !expected.duplicate_reviewer
        && !expected.shared_context;
    assert_eq!(actual.is_satisfied(), satisfied, "rows={rows:?}");
    let reversed = evaluate_release(
        candidate(1),
        10,
        &ReleaseEvidence::new(vec![], vec![], reviews.into_iter().rev().collect(), vec![], vec![])
            .unwrap(),
    );
    assert_eq!(decision, reversed, "input order changed the policy verdict");
}

#[test]
fn fixed_independence_and_quorum_cases() {
    check(&[]);
    check(&[(0, 1, 1), (0, 2, 2)]);
    check(&[(0, 1, 1), (0, 1, 2)]);
    check(&[(0, 1, 1), (0, 2, 1)]);
    check(&[(0, 1, 1), (2, 2, 2)]);
    check(&[(0, 1, 1), (3, 2, 2)]);
    check(&[(0, 1, 1), (4, 2, 2)]);
    check(&[(0, 1, 1), (5, 2, 2)]);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1024, max_shrink_iters: 20_000, failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::WithSource("proptest-regressions"))), .. ProptestConfig::default() })]
    #[test]
    fn generated_review_histories(rows in prop::collection::vec((0u8..6, 1u8..5, 1u8..5), 0..15)) {
        check(&rows);
    }
}

// Independent fields deliberately overlap. Unlike the older exclusive `kind`
// cases, one observation can be self-authored, non-independent, stale and changed.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ReviewRow {
    id: u8,
    reviewer: u8,
    producer: u8,
    context: u8,
    digest: u8,
    sequence: u8,
    observed: u8,
    expires: u8,
    mismatched: bool,
    independent: bool,
    approved: bool,
}

fn row_strategy() -> BoxedStrategy<ReviewRow> {
    (
        (1u8..5, 1u8..5, 1u8..5, 1u8..5, 1u8..5, 1u8..5),
        (8u8..=12, 8u8..=12),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            |(
                (id, reviewer, producer, context, digest, sequence),
                (a, b),
                mismatched,
                independent,
                approved,
            )| ReviewRow {
                id,
                reviewer,
                producer,
                context,
                digest,
                sequence,
                observed: a.min(b),
                expires: a.max(b),
                mismatched,
                independent,
                approved,
            },
        )
        .boxed()
}

fn check_rows(rows: &[ReviewRow]) {
    let observations: Vec<_> = rows
        .iter()
        .map(|row| {
            ReviewObservation::new(
                ReviewId::new([row.id; 16]).unwrap(),
                EvidenceBinding::new(
                    candidate(1),
                    u64::from(row.observed),
                    u64::from(row.expires),
                    u64::from(row.sequence),
                    if row.mismatched { 2 } else { 1 },
                )
                .unwrap(),
                PrincipalId::new([row.reviewer; 16]).unwrap(),
                PrincipalId::new([row.producer; 16]).unwrap(),
                d(row.context),
                d(row.digest),
                if row.approved { ReviewOutcome::Approved } else { ReviewOutcome::ChangesRequired },
                row.independent,
            )
            .unwrap()
        })
        .collect();
    let evaluate = |observations| {
        evaluate_release(
            candidate(1),
            10,
            &ReleaseEvidence::new(vec![], vec![], observations, vec![], vec![]).unwrap(),
        )
    };
    let decision = evaluate(observations.clone());
    let actual = decision.reviews();
    let current: Vec<_> = rows
        .iter()
        .filter(|row| !row.mismatched && row.observed <= 10 && row.expires >= 10)
        .collect();
    let count = |predicate: fn(&ReviewRow) -> bool| -> u16 {
        u16::try_from(current.iter().filter(|row| predicate(row)).count()).unwrap()
    };
    let approved = count(|row| row.approved && row.independent && row.reviewer != row.producer);
    let self_authored = count(|row| row.reviewer == row.producer);
    let non_independent = count(|row| !row.independent);
    let changes = count(|row| !row.approved);
    let mismatched = u16::try_from(rows.iter().filter(|row| row.mismatched).count()).unwrap();
    let stale = u16::try_from(
        rows.iter()
            .filter(|row| !row.mismatched && !(row.observed <= 10 && row.expires >= 10))
            .count(),
    )
    .unwrap();
    let mut duplicate_reviewer = false;
    let mut shared_context = false;
    let mut conflicting = false;
    // Compare raw generated records; do not call production equality/freshness helpers.
    for (index, row) in current.iter().enumerate() {
        for other in &current[..index] {
            duplicate_reviewer |= row.reviewer == other.reviewer;
            shared_context |= row.context == other.context;
            conflicting |= row.id == other.id && row != other;
        }
    }
    assert_eq!(actual.approved_count(), approved, "rows={rows:?}");
    assert_eq!(actual.self_review_count(), self_authored, "rows={rows:?}");
    assert_eq!(actual.non_independent_count(), non_independent, "rows={rows:?}");
    assert_eq!(actual.changes_required_count(), changes, "rows={rows:?}");
    assert_eq!(actual.mismatched_count(), mismatched, "rows={rows:?}");
    assert_eq!(actual.stale_count(), stale, "rows={rows:?}");
    assert_eq!(actual.has_duplicate_reviewer(), duplicate_reviewer, "rows={rows:?}");
    assert_eq!(actual.has_shared_context(), shared_context, "rows={rows:?}");
    assert_eq!(actual.has_conflicting_review(), conflicting, "rows={rows:?}");
    assert_eq!(
        actual.is_satisfied(),
        approved >= 2
            && self_authored == 0
            && non_independent == 0
            && changes == 0
            && mismatched == 0
            && stale == 0
            && !duplicate_reviewer
            && !shared_context
            && !conflicting,
        "rows={rows:?}"
    );
    assert_eq!(decision, evaluate(observations.iter().rev().copied().collect()), "rows={rows:?}");
    if !observations.is_empty() {
        let mut rotated = observations;
        rotated.rotate_left(1);
        assert_eq!(decision, evaluate(rotated), "rows={rows:?}");
    }
}

fn valid_row() -> ReviewRow {
    ReviewRow {
        id: 1,
        reviewer: 1,
        producer: 4,
        context: 1,
        digest: 1,
        sequence: 1,
        observed: 10,
        expires: 10,
        mismatched: false,
        independent: true,
        approved: true,
    }
}

fn changed_row(mut row: ReviewRow, field: u8) -> ReviewRow {
    let other = |value: u8| value % 4 + 1;
    match field {
        0 => row.reviewer = other(row.reviewer),
        1 => row.producer = other(row.producer),
        2 => row.context = other(row.context),
        3 => row.digest = other(row.digest),
        4 => row.sequence = other(row.sequence),
        5 => row.observed = 9,
        6 => row.expires = 11,
        7 => row.independent = !row.independent,
        8 => row.approved = !row.approved,
        _ => unreachable!(),
    }
    row
}

#[test]
fn review_identity_collision_fields_and_inclusive_time() {
    let first = valid_row();
    check_rows(std::slice::from_ref(&first)); // zero-width window is current at its exact tick
    check_rows(&[first.clone(), first.clone()]); // duplicate is not a conflicting identity
    for field in 0..9 {
        check_rows(&[first.clone(), changed_row(first.clone(), field)]);
    }
    check_rows(&[first.clone(), ReviewRow { id: 2, reviewer: 2, context: 2, ..first }]);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1024, max_shrink_iters: 20_000, failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::WithSource("proptest-regressions"))), .. ProptestConfig::default() })]
    #[test]
    fn colliding_reviews_with_overlapping_faults(rows in prop::collection::vec(row_strategy(), 0..24)) {
        check_rows(&rows);
    }
    #[test]
    fn near_valid_review_identity_collisions(field in 0u8..9,
        suffix in prop::collection::vec(row_strategy(), 0..12)) {
        let first = valid_row();
        let mut rows = vec![first.clone(), changed_row(first, field)];
        rows.extend(suffix);
        check_rows(&rows);
    }
}
