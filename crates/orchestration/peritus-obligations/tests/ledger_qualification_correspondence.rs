//! Production-boundary regressions for exact ledger extraction and qualification accounting.

mod support;

use peritus_obligations::{
    AlternativeBranchId, AlternativeGroupId, ConditionId, ConditionObservation, ConditionState,
    DirectEvidence, EvidenceBinding, ObligationErrorKind, ObligationLimits, ObligationSpec,
    PathMention, PathRole, PublicTaskSource, QualificationReport, RequirementDraft,
    RequirementEvidence, RequirementLedger, qualify,
};
use peritus_spec::RequirementId;
use peritus_types::Sha256Digest;
use support::{binding, candidate, digest, ledger, path_id, requirement_id};

const fn trailing_id(last: u8) -> RequirementId {
    let mut bytes = [0; 32];
    bytes[31] = last;
    RequirementId::new(Sha256Digest::new(bytes))
}

fn direct(
    ledger: &RequirementLedger,
    candidate: peritus_run_settlement::CandidateIdentity,
    requirement: u8,
    paths: Vec<peritus_obligations::PathId>,
    satisfied: bool,
) -> RequirementEvidence {
    RequirementEvidence::Direct(DirectEvidence::new(
        binding(ledger, candidate, requirement, paths, requirement.wrapping_add(90)),
        satisfied,
    ))
}

fn assert_report_copy(report: &QualificationReport) {
    let copy = report.clone();
    assert_eq!(copy, *report);
    assert_eq!(copy.unsatisfied_requirements(), report.unsatisfied_requirements());
    assert_eq!(copy.unresolved_conditions(), report.unresolved_conditions());
    assert_eq!(copy.incomplete_alternatives(), report.incomplete_alternatives());
}

#[test]
fn extraction_copies_binary_spans_and_matches_the_complete_identity() {
    let limits = ObligationLimits::production();
    let source =
        PublicTaskSource::new(vec![9, 0, 0xff, 8, 0x80, 1], 41, limits).expect("binary source");
    let first = trailing_id(1);
    let second = trailing_id(2);
    let drafts = vec![
        RequirementDraft::new(first, 1, 3, ObligationSpec::Hard, Vec::new()),
        RequirementDraft::new(second, 4, 6, ObligationSpec::GeneratedOutput, Vec::new()),
    ];

    let ledger = RequirementLedger::extract(&source, drafts, limits).expect("exact extraction");
    assert_eq!(ledger.entries().len(), 2);
    assert_eq!(ledger.entry(first).expect("first identity").clause().exact(), &[0, 0xff]);
    assert_eq!(ledger.entry(second).expect("second identity").clause().exact(), &[0x80, 1]);
    assert!(ledger.entry(trailing_id(0)).is_none());
    assert!(ledger.entry(trailing_id(3)).is_none());

    for (ordinal, entry) in ledger.entries().iter().enumerate() {
        let provenance = entry.clause().provenance();
        assert_eq!(provenance.source_digest(), source.digest());
        assert_eq!(provenance.conversation_revision(), 41);
        assert_eq!(provenance.ordinal(), u32::try_from(ordinal).expect("two entries"));
    }
    assert_eq!(ledger.clone(), ledger);
}

#[test]
fn qualification_report_partitions_actual_verdicts_and_alternative_completion() {
    let condition = ConditionId::new(digest(40));
    let group = AlternativeGroupId::new(digest(50));
    let branch_a = AlternativeBranchId::new(digest(51));
    let branch_b = AlternativeBranchId::new(digest(52));
    let required_output =
        PathMention::new(path_id(61), b"dist/result.bin".to_vec(), PathRole::RequiredOutput, 128)
            .expect("required output");
    let ledger = ledger(vec![
        (1, b"Missing ordinary evidence.", ObligationSpec::Hard, Vec::new()),
        (2, b"Stale ordinary evidence.", ObligationSpec::Hard, Vec::new()),
        (
            3,
            b"Current evidence must name the candidate output.",
            ObligationSpec::GeneratedOutput,
            vec![required_output],
        ),
        (
            4,
            b"An unresolved condition remains fail closed.",
            ObligationSpec::Conditional { condition_id: condition },
            Vec::new(),
        ),
        (
            5,
            b"Alternative branch A.",
            ObligationSpec::Alternative { group_id: group, branch_id: branch_a },
            Vec::new(),
        ),
        (
            6,
            b"Alternative branch B.",
            ObligationSpec::Alternative { group_id: group, branch_id: branch_b },
            Vec::new(),
        ),
    ]);
    let current_candidate = candidate(11, 7, 2);
    let evidence = vec![
        direct(&ledger, candidate(10, 7, 1), 2, Vec::new(), true),
        direct(&ledger, current_candidate, 3, Vec::new(), true),
        direct(&ledger, current_candidate, 6, Vec::new(), true),
    ];
    let conditions = [ConditionObservation::new(condition, ConditionState::Unknown, digest(70))];

    let report = qualify(&ledger, &current_candidate, &conditions, &evidence)
        .expect("canonical supplied inputs");
    assert!(!report.qualified());
    assert_eq!(report.required_count(), 4);
    assert_eq!(report.satisfied_count(), 1);
    assert_eq!(report.missing_count(), 1);
    assert_eq!(report.stale_count(), 1);
    assert_eq!(report.invalid_count(), 1);
    assert_eq!(
        report.unsatisfied_requirements(),
        &[requirement_id(1), requirement_id(2), requirement_id(3)]
    );
    assert_eq!(report.unresolved_conditions(), &[condition]);
    assert!(report.incomplete_alternatives().is_empty());
    assert_report_copy(&report);
}

#[test]
fn qualification_rejects_inputs_in_the_existing_public_error_order() {
    let ledger = ledger(vec![(1, b"One hard requirement.", ObligationSpec::Hard, Vec::new())]);
    let candidate = candidate(11, 7, 2);
    let descending_conditions = [
        ConditionObservation::new(ConditionId::new(digest(2)), ConditionState::Holds, digest(20)),
        ConditionObservation::new(ConditionId::new(digest(1)), ConditionState::Holds, digest(21)),
    ];
    let unknown = RequirementEvidence::Direct(DirectEvidence::new(
        EvidenceBinding::new(
            requirement_id(9),
            ledger.digest(),
            candidate,
            digest(99),
            Vec::new(),
            ledger.limits(),
        )
        .expect("unknown binding"),
        true,
    ));
    let condition_error =
        qualify(&ledger, &candidate, &descending_conditions, core::slice::from_ref(&unknown))
            .expect_err("condition ordering is checked first");
    assert_eq!(condition_error.kind(), ObligationErrorKind::NonCanonicalOrder);

    let duplicate = direct(&ledger, candidate, 1, Vec::new(), true);
    let duplicate_error = qualify(&ledger, &candidate, &[], &[duplicate.clone(), duplicate])
        .expect_err("duplicate evidence");
    assert_eq!(duplicate_error.kind(), ObligationErrorKind::DuplicateValue);

    let unknown_error =
        qualify(&ledger, &candidate, &[], &[unknown]).expect_err("unknown evidence identity");
    assert_eq!(unknown_error.kind(), ObligationErrorKind::UnknownRequirement);
    assert_eq!(unknown_error.requirement_id(), Some(requirement_id(9)));
}

#[test]
fn incomplete_alternative_groups_keep_first_ledger_occurrence_order() {
    let group_late = AlternativeGroupId::new(digest(82));
    let group_early = AlternativeGroupId::new(digest(81));
    let branch_a = AlternativeBranchId::new(digest(91));
    let branch_b = AlternativeBranchId::new(digest(92));
    let ledger = ledger(vec![
        (
            1,
            b"Late group first branch.",
            ObligationSpec::Alternative { group_id: group_late, branch_id: branch_a },
            Vec::new(),
        ),
        (
            2,
            b"Early group first branch.",
            ObligationSpec::Alternative { group_id: group_early, branch_id: branch_a },
            Vec::new(),
        ),
        (
            3,
            b"Late group second branch.",
            ObligationSpec::Alternative { group_id: group_late, branch_id: branch_b },
            Vec::new(),
        ),
        (
            4,
            b"Early group second branch.",
            ObligationSpec::Alternative { group_id: group_early, branch_id: branch_b },
            Vec::new(),
        ),
    ]);

    let report = qualify(&ledger, &candidate(11, 7, 2), &[], &[]).expect("empty evidence report");
    assert_eq!(report.required_count(), 2);
    assert_eq!(report.satisfied_count(), 0);
    assert_eq!(report.incomplete_alternatives(), &[group_late, group_early]);
    assert_eq!((report.missing_count(), report.stale_count(), report.invalid_count()), (0, 0, 0));
}
