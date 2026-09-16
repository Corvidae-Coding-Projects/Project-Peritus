//! Exact identity and ordered-path semantics at the public evidence boundary.

mod support;

use peritus_obligations::{EvidenceBinding, ObligationErrorKind, ObligationLimits, PathId};
use peritus_spec::RequirementId;
use peritus_types::Sha256Digest;
use support::{candidate, digest, requirement_id};

#[test]
fn evidence_currentness_checks_every_requirement_and_ledger_byte() {
    let observed = candidate(11, 7, 2);
    let binding = EvidenceBinding::new(
        requirement_id(1),
        digest(2),
        observed,
        digest(3),
        Vec::new(),
        ObligationLimits::production(),
    )
    .expect("binding");
    assert!(binding.is_current_for(requirement_id(1), digest(2), &observed));
    assert!(binding.is_current_for(requirement_id(1), digest(2), &candidate(11, 7, 3)));
    assert!(!binding.is_current_for(requirement_id(1), digest(2), &candidate(11, 7, 1)));
    for index in 0..32 {
        let mut requirement = [1; 32];
        requirement[index] = 9;
        assert!(!binding.is_current_for(
            RequirementId::new(Sha256Digest::new(requirement)),
            digest(2),
            &observed
        ));
        let mut ledger = [2; 32];
        ledger[index] = 9;
        assert!(!binding.is_current_for(requirement_id(1), Sha256Digest::new(ledger), &observed));
    }
}

#[test]
fn canonical_paths_distinguish_the_last_byte_and_clone_retains_them() {
    let path = |last| {
        let mut bytes = [0; 32];
        bytes[31] = last;
        PathId::new(Sha256Digest::new(bytes))
    };
    let create = |paths| {
        EvidenceBinding::new(
            requirement_id(1),
            digest(2),
            candidate(11, 7, 2),
            digest(3),
            paths,
            ObligationLimits::production(),
        )
    };
    let binding = create(vec![path(2), path(4), path(8)]).expect("ordered paths");
    for present in [2, 4, 8] {
        assert!(binding.contains_path(path(present)));
    }
    for absent in [0, 3, 6, 9] {
        assert!(!binding.contains_path(path(absent)));
    }
    let cloned = binding.clone();
    assert_eq!(cloned, binding);
    assert_eq!(cloned.observed_candidate_paths(), &[path(2), path(4), path(8)]);
    assert_eq!(
        create(vec![path(4), path(2)]).expect_err("unordered").kind(),
        ObligationErrorKind::NonCanonicalOrder
    );
    assert_eq!(
        create(vec![path(2), path(2)]).expect_err("duplicate").kind(),
        ObligationErrorKind::DuplicateValue
    );
}
