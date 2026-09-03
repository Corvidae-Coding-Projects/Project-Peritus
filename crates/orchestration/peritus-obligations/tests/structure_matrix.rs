//! Exact-clause, path-role, conditional, and alternative qualification fixtures.

mod support;

use peritus_obligations::{
    AlternativeBranchId, AlternativeGroupId, ConditionId, ConditionObservation, ConditionState,
    DirectEvidence, ObligationSpec, PathMention, PathRole, RequirementEvidence, qualify,
};

use support::{binding, candidate, digest, ledger, path_id};

#[test]
fn ledger_retains_exact_clause_and_public_provenance() {
    let clause = b"Create exactly dist/result.json and keep source/input.json unchanged.";
    let ledger = ledger(vec![(1, clause, ObligationSpec::GeneratedOutput, Vec::new())]);
    let retained = ledger.entries()[0].clause();

    assert_eq!(retained.exact(), clause);
    assert_eq!(retained.provenance().source_digest(), ledger.source_digest());
    assert_eq!(retained.provenance().conversation_revision(), 7);
    assert_eq!(retained.provenance().byte_start(), 0);
    assert_eq!(retained.provenance().byte_end(), clause.len());
}

#[test]
fn conditional_output_activates_only_when_public_condition_holds() {
    let condition_id = ConditionId::new(digest(21));
    let output = PathMention::new(
        path_id(31),
        b"dist/conditional.json".to_vec(),
        PathRole::RequiredOutput,
        256,
    )
    .expect("output path");
    let reference = PathMention::new(
        path_id(32),
        b"examples/reference.json".to_vec(),
        PathRole::Reference,
        256,
    )
    .expect("reference path");
    let ledger = ledger(vec![(
        1,
        b"When JSON mode is enabled, create dist/conditional.json like examples/reference.json.",
        ObligationSpec::Conditional { condition_id },
        vec![output, reference],
    )]);
    let candidate = candidate(11, 7, 2);

    let false_condition =
        [ConditionObservation::new(condition_id, ConditionState::DoesNotHold, digest(41))];
    assert!(
        qualify(&ledger, &candidate, &false_condition, &[]).expect("inactive report").qualified()
    );

    let unknown = [ConditionObservation::new(condition_id, ConditionState::Unknown, digest(42))];
    let unknown_report = qualify(&ledger, &candidate, &unknown, &[]).expect("unknown report");
    assert!(!unknown_report.qualified());
    assert_eq!(unknown_report.unresolved_conditions(), &[condition_id]);

    let true_condition =
        [ConditionObservation::new(condition_id, ConditionState::Holds, digest(43))];
    assert!(
        !qualify(&ledger, &candidate, &true_condition, &[]).expect("missing report").qualified()
    );

    let direct = RequirementEvidence::Direct(DirectEvidence::new(
        binding(&ledger, candidate, 1, vec![path_id(31)], 44),
        true,
    ));
    assert!(
        qualify(&ledger, &candidate, &true_condition, &[direct])
            .expect("active report")
            .qualified()
    );
}

#[test]
fn referenced_and_example_paths_are_not_mandatory_outputs() {
    let reference =
        PathMention::new(path_id(31), b"docs/reference.md".to_vec(), PathRole::RequiredInput, 256)
            .expect("input path");
    let example =
        PathMention::new(path_id(32), b"examples/output.md".to_vec(), PathRole::Example, 256)
            .expect("example path");
    let ledger = ledger(vec![(
        1,
        b"Read docs/reference.md; examples/output.md only illustrates the format.",
        ObligationSpec::Hard,
        vec![reference, example],
    )]);
    let candidate = candidate(11, 7, 2);
    let direct = RequirementEvidence::Direct(DirectEvidence::new(
        binding(&ledger, candidate, 1, Vec::new(), 45),
        true,
    ));
    assert!(qualify(&ledger, &candidate, &[], &[direct]).expect("path report").qualified());
}

#[test]
fn example_clauses_are_informative_without_candidate_evidence() {
    let ledger = ledger(vec![(
        1,
        b"For example, a caller might choose compact JSON.",
        ObligationSpec::Example,
        Vec::new(),
    )]);

    let report = qualify(&ledger, &candidate(11, 7, 2), &[], &[]).expect("example report");
    assert!(report.qualified());
    assert_eq!(report.required_count(), 0);
}

#[test]
fn one_complete_alternative_branch_is_required() {
    let group_id = AlternativeGroupId::new(digest(51));
    let branch_a = AlternativeBranchId::new(digest(52));
    let branch_b = AlternativeBranchId::new(digest(53));
    let ledger = ledger(vec![
        (
            1,
            b"Branch A requires format A.",
            ObligationSpec::Alternative { group_id, branch_id: branch_a },
            Vec::new(),
        ),
        (
            2,
            b"Branch A also requires transport A.",
            ObligationSpec::Alternative { group_id, branch_id: branch_a },
            Vec::new(),
        ),
        (
            3,
            b"Alternatively branch B requires its complete native format.",
            ObligationSpec::Alternative { group_id, branch_id: branch_b },
            Vec::new(),
        ),
    ]);
    let candidate = candidate(11, 7, 2);
    let partial_a = RequirementEvidence::Direct(DirectEvidence::new(
        binding(&ledger, candidate, 1, Vec::new(), 61),
        true,
    ));
    let incomplete = qualify(&ledger, &candidate, &[], &[partial_a]).expect("incomplete report");
    assert!(!incomplete.qualified());
    assert_eq!(incomplete.incomplete_alternatives(), &[group_id]);

    let complete_b = RequirementEvidence::Direct(DirectEvidence::new(
        binding(&ledger, candidate, 3, Vec::new(), 62),
        true,
    ));
    assert!(qualify(&ledger, &candidate, &[], &[complete_b]).expect("branch report").qualified());
}
