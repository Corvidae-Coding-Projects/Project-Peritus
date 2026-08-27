//! H0 through H3 qualification input contracts.

mod support;

use peritus_release_policy::{
    Diagnostic, QualificationObservation, QualificationSlice, QualificationVerdict, ReleaseVerdict,
};
use support::{binding, digest, principal, ready_inputs, stale_binding};

#[test]
fn every_h_slice_is_required() {
    for slice in QualificationSlice::ALL {
        let mut inputs = ready_inputs();
        inputs.qualifications.retain(|value| value.slice() != slice);
        let decision = inputs.evaluate();
        assert_eq!(decision.verdict(), ReleaseVerdict::NotReadyForProduction);
        assert!(decision.diagnostics().contains(&Diagnostic::MissingQualification(slice)));
    }
}

#[test]
fn explicit_not_ready_qualification_blocks_release() {
    let mut inputs = ready_inputs();
    let candidate = inputs.candidate;
    inputs.qualifications.retain(|value| value.slice() != QualificationSlice::H0Security);
    inputs.qualifications.push(
        QualificationObservation::new(
            QualificationSlice::H0Security,
            binding(&candidate, 600),
            QualificationVerdict::NotReadyForProduction,
            digest(220),
            digest(221),
            principal(70),
            true,
        )
        .expect("not-ready qualification"),
    );
    let decision = inputs.evaluate();
    assert!(
        decision
            .diagnostics()
            .contains(&Diagnostic::QualificationNotReady(QualificationSlice::H0Security, 1,))
    );
    assert!(
        decision
            .diagnostics()
            .contains(&Diagnostic::MissingQualification(QualificationSlice::H0Security,))
    );
}

#[test]
fn stale_and_unreviewed_qualifications_never_count_as_ready() {
    let mut inputs = ready_inputs();
    let candidate = inputs.candidate;
    inputs.qualifications.retain(|value| value.slice() != QualificationSlice::H3Performance);
    inputs.qualifications.extend([
        QualificationObservation::new(
            QualificationSlice::H3Performance,
            stale_binding(&candidate, 601),
            QualificationVerdict::Ready,
            digest(222),
            digest(223),
            principal(71),
            true,
        )
        .expect("stale qualification"),
        QualificationObservation::new(
            QualificationSlice::H3Performance,
            binding(&candidate, 602),
            QualificationVerdict::Ready,
            digest(224),
            digest(225),
            principal(72),
            false,
        )
        .expect("unreviewed qualification"),
    ]);
    let decision = inputs.evaluate();
    assert!(
        decision
            .diagnostics()
            .contains(&Diagnostic::StaleQualification(QualificationSlice::H3Performance, 1,))
    );
    assert!(
        decision
            .diagnostics()
            .contains(&Diagnostic::UnreviewedQualification(QualificationSlice::H3Performance, 1,))
    );
}

#[test]
fn conflicting_current_qualification_reports_block_release() {
    let mut inputs = ready_inputs();
    let candidate = inputs.candidate;
    inputs.qualifications.push(
        QualificationObservation::new(
            QualificationSlice::H2Platform,
            binding(&candidate, 603),
            QualificationVerdict::Ready,
            digest(226),
            digest(227),
            principal(73),
            true,
        )
        .expect("conflicting qualification"),
    );
    let decision = inputs.evaluate();
    assert!(
        decision
            .diagnostics()
            .contains(&Diagnostic::ConflictingQualification(QualificationSlice::H2Platform,))
    );
}
