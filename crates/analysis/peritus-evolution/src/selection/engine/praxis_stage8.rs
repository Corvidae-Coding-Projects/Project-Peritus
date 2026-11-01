use super::*;
use crate::{
    AttributionId, Criterion, CriterionResult, EvolutionLimits, PromotionThresholds, VariantId,
};
use proptest::prelude::*;

const OBJECTIVES: [Objective; 9] = [
    Objective::PairedCorrectness,
    Objective::CriticalRegressions,
    Objective::SafetyFailures,
    Objective::Reliability,
    Objective::Latency,
    Objective::Cost,
    Objective::InputTokens,
    Objective::OutputTokens,
    Objective::AttributionCoverage,
];

const CRITERIA: [Criterion; 14] = [
    Criterion::PairedCorrectness,
    Criterion::CriticalRegressions,
    Criterion::Safety,
    Criterion::Reliability,
    Criterion::AttributionCoverage,
    Criterion::MandatoryPredictions,
    Criterion::Latency,
    Criterion::Cost,
    Criterion::InputTokens,
    Criterion::OutputTokens,
    Criterion::TraceCompleteness,
    Criterion::TeardownCompleteness,
    Criterion::IndependentReview,
    Criterion::Compatibility,
];

fn policy(objective: Objective) -> PromotionPolicy {
    ordered_policy(vec![objective])
}

fn ordered_policy(objectives: Vec<Objective>) -> PromotionPolicy {
    let thresholds = PromotionThresholds::new(
        -1_000_000,
        u32::MAX,
        u32::MAX,
        0,
        0,
        u64::MAX,
        u64::MAX,
        u64::MAX,
        u64::MAX,
        false,
        false,
    )
    .unwrap();
    PromotionPolicy::new(thresholds, objectives, vec![], true, 128, EvolutionLimits::compiled())
        .unwrap()
}

fn assessment(
    id: u8,
    scores: [u16; 9],
    eligible: bool,
    policy_digest: Sha256Digest,
) -> VariantAssessment {
    let mut outcomes = [0; 14];
    if !eligible {
        outcomes[2] = 1;
    }
    assessment_outcomes(id, scores, outcomes, policy_digest)
}

fn assessment_outcomes(
    id: u8,
    scores: [u16; 9],
    outcomes: [u8; 14],
    policy_digest: Sha256Digest,
) -> VariantAssessment {
    let evidence = Sha256Digest::new([3; 32]);
    let criteria = CRITERIA
        .into_iter()
        .enumerate()
        .map(|(index, criterion)| {
            CriterionResult::new(
                criterion,
                match outcomes[index] {
                    0 => CriterionOutcome::Passed,
                    1 => CriterionOutcome::Failed,
                    _ => CriterionOutcome::Unavailable,
                },
                None,
                evidence,
            )
        })
        .collect();
    VariantAssessment::from_exact_parts(
        VariantId::new([id; 16]).unwrap(),
        AttributionId::new([id; 16]).unwrap(),
        evidence,
        policy_digest,
        criteria,
        ObjectiveVector {
            paired_lower: i32::from(scores[0]),
            critical_regressions: u32::from(scores[1]),
            safety_failures: u32::from(scores[2]),
            reliability_lower: u32::from(scores[3]),
            latency_p95: u64::from(scores[4]),
            cost_mean: u64::from(scores[5]),
            input_tokens_mean: u64::from(scores[6]),
            output_tokens_mean: u64::from(scores[7]),
            attribution_coverage: u32::from(scores[8]),
        },
    )
}

fn independent_rank(objective: Objective, scores: [u16; 9]) -> i64 {
    match objective {
        Objective::PairedCorrectness => -i64::from(scores[0]),
        Objective::CriticalRegressions => i64::from(scores[1]),
        Objective::SafetyFailures => i64::from(scores[2]),
        Objective::Reliability => -i64::from(scores[3]),
        Objective::Latency => i64::from(scores[4]),
        Objective::Cost => i64::from(scores[5]),
        Objective::InputTokens => i64::from(scores[6]),
        Objective::OutputTokens => i64::from(scores[7]),
        Objective::AttributionCoverage => -i64::from(scores[8]),
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    #[test]
    fn selected_variant_matches_independent_single_objective_sort(
        cases in proptest::collection::vec((proptest::array::uniform9(0_u16..100), any::<bool>()), 1..25),
        objective_index in 0_usize..9,
    ) {
        let objective = OBJECTIVES[objective_index];
        let policy = policy(objective);
        let assessments: Vec<_> = cases.iter().enumerate().map(|(index, (scores, eligible))|
            assessment(u8::try_from(index + 1).unwrap(), *scores, *eligible, policy.digest())
        ).collect();
        let observed = select_variant(&policy, &assessments).unwrap();
        let expected = cases.iter().enumerate()
            .filter(|(_, (_, eligible))| *eligible)
            .min_by_key(|(index, (scores, _))| (independent_rank(objective, *scores), *index))
            .map(|(index, _)| VariantId::new([u8::try_from(index + 1).unwrap(); 16]).unwrap());
        match (observed.decision(), expected) {
            (SelectionDecision::Selected(actual), Some(winner)) => prop_assert_eq!(*actual, winner),
            (SelectionDecision::NoEligibleVariant(rejections), None) => {
                prop_assert_eq!(rejections.len(), assessments.len());
                for (rejection, assessment) in rejections.iter().zip(&assessments) {
                    prop_assert_eq!(rejection.variant_id(), assessment.variant_id());
                    prop_assert_eq!(rejection.failed(), &[Criterion::Safety]);
                }
            }
            (other, expected) => prop_assert!(false, "selection mismatch: {other:?} vs {expected:?}"),
        }
    }

    #[test]
    fn selection_uses_every_ordered_objective_and_preserves_rejection_causes(
        cases in prop::collection::vec((prop::array::uniform9(0_u16..=2),
            any::<bool>(), prop::array::uniform14(0_u8..=2)), 1..25),
        order in prop::sample::subsequence(OBJECTIVES.to_vec(), 1..=9)
            .prop_shuffle(),
    ) {
        let policy = ordered_policy(order.clone());
        let inputs = cases.iter().enumerate().map(|(index, (scores, eligible, faults))| {
            let mut outcomes = if *eligible { [0; 14] } else { *faults };
            if !eligible && outcomes.iter().all(|value| *value == 0) { outcomes[0] = 2; }
            (u8::try_from(index * 2 + 1).unwrap(), *scores, outcomes)
        }).collect::<Vec<_>>();
        let assessments = inputs.iter().map(|(id, scores, outcomes)|
            assessment_outcomes(*id, *scores, *outcomes, policy.digest())).collect::<Vec<_>>();
        let expected = inputs.iter().filter(|(_, _, outcomes)| outcomes.iter().all(|value| *value == 0))
            .min_by_key(|(id, scores, _)| (order.iter().map(|objective| independent_rank(*objective, *scores))
                .collect::<Vec<_>>(), *id));
        let observed = select_variant(&policy, &assessments).unwrap();
        match (observed.decision(), expected) {
            (SelectionDecision::Selected(actual), Some((id, _, _))) =>
                prop_assert_eq!(*actual, VariantId::new([*id; 16]).unwrap()),
            (SelectionDecision::NoEligibleVariant(rejections), None) => {
                prop_assert_eq!(rejections.len(), inputs.len());
                for (rejection, (id, _, outcomes)) in rejections.iter().zip(&inputs) {
                    let failed = CRITERIA.iter().enumerate().filter(|(i, _)| outcomes[*i] == 1)
                        .map(|(_, criterion)| *criterion).collect::<Vec<_>>();
                    let unavailable = CRITERIA.iter().enumerate().filter(|(i, _)| outcomes[*i] == 2)
                        .map(|(_, criterion)| *criterion).collect::<Vec<_>>();
                    prop_assert_eq!(rejection.variant_id(), VariantId::new([*id; 16]).unwrap());
                    prop_assert_eq!(rejection.failed(), failed.as_slice());
                    prop_assert_eq!(rejection.unavailable(), unavailable.as_slice());
                }
            }
            (actual, expected) => prop_assert!(false, "selection mismatch: {actual:?} vs {expected:?}"),
        }
        // Legal selection must remain possible after rejecting malformed input order/identity.
        if assessments.len() > 1 {
            let mut reversed = assessments.clone();
            reversed.reverse();
            prop_assert!(select_variant(&policy, &reversed).is_err());
        }
        let duplicate = vec![assessments[0].clone(), assessments[0].clone()];
        prop_assert!(select_variant(&policy, &duplicate).is_err());
        prop_assert_eq!(select_variant(&policy, &assessments).unwrap(), observed);
    }

}

#[test]
fn higher_reliability_wins_with_all_other_scores_equal() {
    let policy = policy(Objective::Reliability);
    let mut low = [0; 9];
    low[3] = 40;
    let mut high = low;
    high[3] = 90;
    let assessments =
        vec![assessment(1, low, true, policy.digest()), assessment(2, high, true, policy.digest())];
    let result = select_variant(&policy, &assessments).unwrap();
    assert_eq!(result.decision(), &SelectionDecision::Selected(VariantId::new([2; 16]).unwrap()));
}
