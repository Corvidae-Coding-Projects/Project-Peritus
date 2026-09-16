//! Public qualification verdict and first-error precedence regressions.

mod support;

use peritus_obligations::{
    AlternativeBranchId, AlternativeGroupId, ConditionId, ConditionObservation, ConditionState,
    DirectEvidence, ObligationError, ObligationErrorKind, ObligationLimits, ObligationSpec,
    PublicTaskSource, RequirementDraft, RequirementEvidence, RequirementLedger, qualify,
};
use peritus_run_settlement::CandidateIdentity;
use peritus_spec::RequirementId;
use support::{binding, candidate, digest, ledger, requirement_id};

fn direct(
    ledger: &RequirementLedger,
    candidate: CandidateIdentity,
    id: u8,
    satisfied: bool,
) -> RequirementEvidence {
    RequirementEvidence::Direct(DirectEvidence::new(
        binding(ledger, candidate, id, Vec::new(), id + 90),
        satisfied,
    ))
}

const fn details(
    error: ObligationError,
) -> (ObligationErrorKind, Option<RequirementId>, Option<u64>, Option<u64>) {
    (error.kind(), error.requirement_id(), error.expected(), error.actual())
}

#[test]
fn qualification_keeps_first_error_and_every_payload_field() {
    let limits = ObligationLimits::new(64, 64, 8, 1, 1, 2).expect("small valid limits");
    let source = PublicTaskSource::new(b"x".to_vec(), 7, limits).expect("source");
    let ledger = RequirementLedger::extract(
        &source,
        vec![RequirementDraft::new(requirement_id(1), 0, 1, ObligationSpec::Hard, Vec::new())],
        limits,
    )
    .expect("one known requirement");
    let candidate = candidate(11, 7, 2);
    let condition = |id| {
        ConditionObservation::new(ConditionId::new(digest(id)), ConditionState::Holds, digest(80))
    };
    let oversized = [
        direct(&ledger, candidate, 3, true),
        direct(&ledger, candidate, 3, true),
        direct(&ledger, candidate, 2, true),
    ];
    for (conditions, kind) in [
        ([condition(1), condition(1), condition(0)], ObligationErrorKind::InvalidCondition),
        ([condition(2), condition(1), condition(1)], ObligationErrorKind::NonCanonicalOrder),
    ] {
        let error = qualify(&ledger, &candidate, &conditions, &oversized)
            .expect_err("first bad condition precedes every evidence error");
        assert_eq!(details(error), (kind, None, None, None));
    }
    let error = qualify(&ledger, &candidate, &[], &oversized)
        .expect_err("bound precedes duplicate, descending and unknown evidence");
    assert_eq!(details(error), (ObligationErrorKind::LimitExceeded, None, Some(2), Some(3)));

    for (ids, expected) in [
        ([3, 1], (ObligationErrorKind::NonCanonicalOrder, None, None, None)),
        ([3, 3], (ObligationErrorKind::DuplicateValue, None, None, None)),
        ([2, 3], (ObligationErrorKind::UnknownRequirement, Some(requirement_id(2)), None, None)),
        ([1, 2], (ObligationErrorKind::UnknownRequirement, Some(requirement_id(2)), None, None)),
    ] {
        let evidence = ids.map(|id| direct(&ledger, candidate, id, true));
        let error = qualify(&ledger, &candidate, &[], &evidence).expect_err("evidence rejected");
        assert_eq!(details(error), expected, "evidence identities {ids:?}");
    }
}

#[test]
fn qualification_requires_every_active_member_and_one_whole_alternative_branch() {
    let condition = ConditionId::new(digest(40));
    let group = AlternativeGroupId::new(digest(50));
    let branch_a = AlternativeBranchId::new(digest(51));
    let branch_b = AlternativeBranchId::new(digest(52));
    let alternative = |branch_id| ObligationSpec::Alternative { group_id: group, branch_id };
    let ledger = ledger(vec![
        (1, b"Always required.", ObligationSpec::Hard, Vec::new()),
        (
            2,
            b"Required when the condition holds.",
            ObligationSpec::Conditional { condition_id: condition },
            Vec::new(),
        ),
        (3, b"An illustrative example.", ObligationSpec::Example, Vec::new()),
        (4, b"Branch A first member.", alternative(branch_a), Vec::new()),
        (5, b"Branch A second member.", alternative(branch_a), Vec::new()),
        (6, b"Branch B first member.", alternative(branch_b), Vec::new()),
        (7, b"Branch B second member.", alternative(branch_b), Vec::new()),
    ]);
    let candidate = candidate(11, 7, 2);
    for state in [
        None,
        Some(ConditionState::Unknown),
        Some(ConditionState::DoesNotHold),
        Some(ConditionState::Holds),
    ] {
        let conditions: Vec<_> = state
            .into_iter()
            .map(|state| ConditionObservation::new(condition, state, digest(70)))
            .collect();
        for hard in [false, true] {
            for conditional in [false, true] {
                for mask in 0..16u8 {
                    let mut evidence = vec![
                        direct(&ledger, candidate, 1, hard),
                        direct(&ledger, candidate, 2, conditional),
                        direct(&ledger, candidate, 3, false),
                    ];
                    for offset in 0..4u8 {
                        evidence.push(direct(
                            &ledger,
                            candidate,
                            offset + 4,
                            mask & (1 << offset) != 0,
                        ));
                    }
                    let report = qualify(&ledger, &candidate, &conditions, &evidence)
                        .expect("canonical observations");
                    let condition_met = state == Some(ConditionState::DoesNotHold)
                        || (state == Some(ConditionState::Holds) && conditional);
                    let whole_branch = mask & 0b0011 == 0b0011 || mask & 0b1100 == 0b1100;
                    assert_eq!(
                        report.qualified(),
                        hard && condition_met && whole_branch,
                        "state={state:?}, hard={hard}, conditional={conditional}, branches={mask:04b}"
                    );
                }
            }
        }
    }
}
