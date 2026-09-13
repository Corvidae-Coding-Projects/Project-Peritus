//! Fail-closed release-policy evaluation contracts.

mod support;

use peritus_release_policy::{
    Diagnostic, EvidenceObservation, EvidenceRequirement, EvidenceSourceKind, ReleaseVerdict,
};
use support::{binding, digest, mismatched_candidate, ready_inputs, stale_binding};

#[test]
fn stale_evidence_is_diagnosed_and_cannot_contribute() {
    let mut inputs = ready_inputs();
    let candidate = inputs.candidate;
    inputs.observations.retain(|value| value.requirement() != EvidenceRequirement::GateA);
    inputs.observations.push(
        EvidenceObservation::new(
            EvidenceRequirement::GateA,
            EvidenceSourceKind::GateA,
            stale_binding(&candidate, 500),
            digest(201),
            digest(202),
            true,
            true,
        )
        .expect("stale observation is structurally valid"),
    );
    let decision = inputs.evaluate();
    assert_eq!(decision.verdict(), ReleaseVerdict::NotReadyForProduction);
    assert!(
        decision.diagnostics().contains(&Diagnostic::StaleEvidence(EvidenceRequirement::GateA, 1,))
    );
    assert!(
        decision.diagnostics().contains(&Diagnostic::MissingEvidence(EvidenceRequirement::GateA,))
    );
}

#[test]
fn mismatched_candidate_evidence_is_diagnosed_and_excluded() {
    let mut inputs = ready_inputs();
    inputs.observations.retain(|value| value.requirement() != EvidenceRequirement::GateA);
    let other = mismatched_candidate();
    inputs.observations.push(
        EvidenceObservation::new(
            EvidenceRequirement::GateA,
            EvidenceSourceKind::GateA,
            binding(&other, 501),
            digest(203),
            digest(204),
            true,
            true,
        )
        .expect("mismatched observation is structurally valid"),
    );
    let decision = inputs.evaluate();
    assert!(
        decision
            .diagnostics()
            .contains(&Diagnostic::MismatchedEvidence(EvidenceRequirement::GateA, 1,))
    );
    assert!(
        decision.diagnostics().contains(&Diagnostic::MissingEvidence(EvidenceRequirement::GateA,))
    );
}

#[test]
fn wrong_source_unreviewed_unsigned_and_conflict_are_distinct() {
    let mut inputs = ready_inputs();
    let candidate = inputs.candidate;
    inputs.observations.retain(|value| value.requirement() != EvidenceRequirement::GateA);
    inputs.observations.extend([
        EvidenceObservation::new(
            EvidenceRequirement::GateA,
            EvidenceSourceKind::Foundation,
            binding(&candidate, 510),
            digest(210),
            digest(211),
            true,
            true,
        )
        .expect("wrong source"),
        EvidenceObservation::new(
            EvidenceRequirement::GateA,
            EvidenceSourceKind::GateA,
            binding(&candidate, 511),
            digest(212),
            digest(213),
            false,
            true,
        )
        .expect("unreviewed"),
        EvidenceObservation::new(
            EvidenceRequirement::GateA,
            EvidenceSourceKind::GateA,
            binding(&candidate, 512),
            digest(214),
            digest(1),
            true,
            false,
        )
        .expect("unsigned"),
        EvidenceObservation::new(
            EvidenceRequirement::GateA,
            EvidenceSourceKind::GateA,
            binding(&candidate, 513),
            digest(215),
            digest(216),
            true,
            true,
        )
        .expect("first current"),
        EvidenceObservation::new(
            EvidenceRequirement::GateA,
            EvidenceSourceKind::GateA,
            binding(&candidate, 514),
            digest(217),
            digest(218),
            true,
            true,
        )
        .expect("conflicting current"),
    ]);
    let decision = inputs.evaluate();
    assert!(
        decision
            .diagnostics()
            .contains(&Diagnostic::WrongEvidenceSource(EvidenceRequirement::GateA, 1,))
    );
    assert!(
        decision
            .diagnostics()
            .contains(&Diagnostic::UnreviewedEvidence(EvidenceRequirement::GateA, 1,))
    );
    assert!(
        decision
            .diagnostics()
            .contains(&Diagnostic::UnsignedEvidence(EvidenceRequirement::GateA, 1,))
    );
    assert!(
        decision
            .diagnostics()
            .contains(&Diagnostic::ConflictingEvidence(EvidenceRequirement::GateA,))
    );
}
