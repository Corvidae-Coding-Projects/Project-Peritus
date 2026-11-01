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
    #![proptest_config(ProptestConfig { cases: 1024, max_shrink_iters: 20_000, .. ProptestConfig::default() })]
    #[test]
    fn generated_review_histories(rows in prop::collection::vec((0u8..6, 1u8..5, 1u8..5), 0..15)) {
        check(&rows);
    }
}
