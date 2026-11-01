use peritus_release_policy::{
    Architecture, CandidateId, EvidenceBinding, EvidenceObservation, EvidenceRequirement,
    EvidenceSourceKind, GitCommitId, OperatingSystem, PlatformIdentity, PlatformMatrix,
    ProfileIdentity, ReleaseCandidate, ReleaseEvidence, ReleaseVersion, SchemaIdentity,
    ToolchainIdentity, evaluate_release,
};
use peritus_types::Sha256Digest;
use proptest::prelude::*;

fn digest(tag: u8) -> Sha256Digest {
    Sha256Digest::new([tag.max(1); 32])
}

fn candidate(id: u8) -> ReleaseCandidate {
    let platform = |os| PlatformIdentity::new(os, Architecture::X86_64, digest(3)).unwrap();
    ReleaseCandidate::new(
        CandidateId::new([id; 16]).unwrap(),
        GitCommitId::sha1([2; 20]).unwrap(),
        ReleaseVersion::new(0, 1, 0, digest(4)).unwrap(),
        PlatformMatrix::new(
            platform(OperatingSystem::Linux),
            platform(OperatingSystem::MacOs),
            platform(OperatingSystem::Windows),
        )
        .unwrap(),
        ToolchainIdentity::new(digest(5), digest(6), digest(7), digest(8)).unwrap(),
        ProfileIdentity::new(1, digest(9)).unwrap(),
        SchemaIdentity::new(1, 1, 1, 1, digest(10)).unwrap(),
        1,
        digest(11),
    )
    .unwrap()
}

fn observation(kind: u8, tag: u8) -> EvidenceObservation {
    let observed_candidate = candidate(if kind == 3 { 2 } else { 1 });
    let (observed_at, expires_at) = match kind {
        1 => (11, 20), // future
        2 => (1, 9),   // expired
        _ => (1, 20),
    };
    let binding = EvidenceBinding::new(
        observed_candidate,
        observed_at,
        expires_at,
        1,
        if kind == 4 { 2 } else { 1 },
    )
    .unwrap();
    let requirement = if kind == 8 {
        EvidenceRequirement::FoundationQualityMatrix
    } else {
        EvidenceRequirement::GateA
    };
    EvidenceObservation::new(
        requirement,
        if kind == 5 { EvidenceSourceKind::Foundation } else { EvidenceSourceKind::GateA },
        binding,
        digest(tag),
        digest(20),
        kind != 6 && kind != 9,
        kind != 7 && kind != 9,
    )
    .unwrap()
}

#[derive(Default, Debug, PartialEq, Eq)]
struct Counts {
    contributing: u16,
    stale: u16,
    mismatched: u16,
    wrong_source: u16,
    unreviewed: u16,
    unsigned: u16,
    conflicting: bool,
    digest: [u8; 32],
}

fn expected(rows: &[(u8, u8)]) -> Counts {
    let mut counts = Counts::default();
    let mut first = None;
    for &(kind, tag) in rows {
        match kind {
            0 => {}
            1 | 2 => counts.stale += 1,
            3 | 4 => counts.mismatched += 1,
            5 => counts.wrong_source += 1,
            6 => counts.unreviewed += 1,
            7 => counts.unsigned += 1,
            8 => continue,
            9 => {
                counts.unreviewed += 1;
                counts.unsigned += 1;
            }
            _ => unreachable!(),
        }
        if kind == 0 {
            counts.contributing += 1;
            let bytes = *digest(tag).as_bytes();
            if first.is_some_and(|value| value != bytes) {
                counts.conflicting = true;
            }
            first.get_or_insert(bytes);
            for (out, byte) in counts.digest.iter_mut().zip(bytes) {
                *out ^= byte;
            }
        }
    }
    counts
}

fn check(rows: &[(u8, u8)]) {
    let raw: Vec<_> = rows.iter().map(|&(kind, tag)| observation(kind, tag)).collect();
    let decision = evaluate_release(
        candidate(1),
        10,
        &ReleaseEvidence::new(raw.clone(), vec![], vec![], vec![], vec![]).unwrap(),
    );
    let actual = decision.evidence()[0];
    let model = expected(rows);
    assert_eq!(actual.requirement(), EvidenceRequirement::GateA);
    assert_eq!(actual.contributing_count(), model.contributing, "rows={rows:?}");
    assert_eq!(actual.stale_count(), model.stale, "rows={rows:?}");
    assert_eq!(actual.mismatched_count(), model.mismatched, "rows={rows:?}");
    assert_eq!(actual.wrong_source_count(), model.wrong_source, "rows={rows:?}");
    assert_eq!(actual.unreviewed_count(), model.unreviewed, "rows={rows:?}");
    assert_eq!(actual.unsigned_count(), model.unsigned, "rows={rows:?}");
    assert_eq!(actual.is_conflicting(), model.conflicting, "rows={rows:?}");
    assert_eq!(actual.contributing_digest().as_bytes(), &model.digest, "rows={rows:?}");
    let satisfied = model.contributing > 0
        && model.stale == 0
        && model.mismatched == 0
        && model.wrong_source == 0
        && model.unreviewed == 0
        && model.unsigned == 0
        && !model.conflicting;
    assert_eq!(actual.is_satisfied(), satisfied, "rows={rows:?}");
    assert!(!decision.is_ready()); // Other 43 requirements and H0-H3 are absent.

    let reversed = evaluate_release(
        candidate(1),
        10,
        &ReleaseEvidence::new(raw.into_iter().rev().collect(), vec![], vec![], vec![], vec![])
            .unwrap(),
    );
    assert_eq!(decision, reversed, "observation permutation changed the decision");
}

#[test]
fn fixed_boundaries() {
    check(&[]);
    check(&[(0, 1)]);
    check(&[(0, 1), (0, 2)]);
    check(&[(1, 1), (2, 1), (3, 1), (4, 1), (5, 1), (6, 1), (7, 1), (8, 1), (9, 1)]);
    check(&[(0, 1), (8, 2), (0, 1)]);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1024, max_shrink_iters: 20_000, .. ProptestConfig::default() })]
    #[test]
    fn generated_evidence_histories(rows in prop::collection::vec((0u8..10, 1u8..5), 0..24)) {
        check(&rows);
    }
}
