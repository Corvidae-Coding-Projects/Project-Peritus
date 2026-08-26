use peritus_harness::domain::ComponentKind;
use peritus_types::Sha256Digest;

use crate::{
    AttributionId, EvolutionErrorKind, EvolutionLimits, Objective, PromotionPolicy,
    PromotionThresholds, VariantId,
};

use super::{
    Criterion, CriterionOutcome, CriterionResult, ObjectiveVector, SelectionDecision,
    SelectionRecord, VariantAssessment, VariantRejection, engine::requires_review, select_variant,
};

#[test]
fn selection_rejects_assessment_from_another_policy() {
    let assessed_policy = policy(false);
    let selecting_policy = policy(true);
    let assessments = [assessment(assessed_policy.digest(), 1)];

    let error = select_variant(&selecting_policy, &assessments)
        .expect_err("an assessment cannot move between frozen policies");

    assert_eq!(error.kind(), EvolutionErrorKind::BindingDrift);
}

#[test]
fn executable_change_requires_review_even_without_a_policy_kind() {
    assert!(requires_review(true, &[ComponentKind::RolePrompt], &[]));
    assert!(!requires_review(false, &[ComponentKind::RolePrompt], &[]));
    assert!(requires_review(false, &[ComponentKind::RolePrompt], &[ComponentKind::RolePrompt],));
}

#[test]
fn failed_and_unavailable_rejection_sections_have_distinct_digests() {
    let variant = variant_id(1);
    let failed = SelectionRecord::from_exact_parts(
        digest(2),
        vec![digest(3)],
        SelectionDecision::NoEligibleVariant(vec![VariantRejection::new(
            variant,
            vec![Criterion::PairedCorrectness],
            Vec::new(),
        )]),
    );
    let unavailable = SelectionRecord::from_exact_parts(
        digest(2),
        vec![digest(3)],
        SelectionDecision::NoEligibleVariant(vec![VariantRejection::new(
            variant,
            Vec::new(),
            vec![Criterion::PairedCorrectness],
        )]),
    );

    assert_ne!(failed.digest(), unavailable.digest());
}

fn policy(allow_cross_lineage: bool) -> PromotionPolicy {
    let thresholds = PromotionThresholds::new(
        0,
        0,
        0,
        0,
        0,
        u64::MAX,
        u64::MAX,
        u64::MAX,
        u64::MAX,
        false,
        false,
    )
    .expect("valid thresholds");
    PromotionPolicy::new(
        thresholds,
        vec![Objective::PairedCorrectness],
        Vec::new(),
        allow_cross_lineage,
        2,
        EvolutionLimits::compiled(),
    )
    .expect("valid policy")
}

fn assessment(policy_digest: Sha256Digest, identity: u8) -> VariantAssessment {
    let criteria = [
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
    ]
    .into_iter()
    .map(|criterion| {
        CriterionResult::new(criterion, CriterionOutcome::Passed, None, digest(identity))
    })
    .collect();
    VariantAssessment::from_exact_parts(
        variant_id(identity),
        AttributionId::new([identity.saturating_add(1); 16]).expect("nonzero attribution"),
        digest(identity.saturating_add(2)),
        policy_digest,
        criteria,
        ObjectiveVector {
            paired_lower: 0,
            critical_regressions: 0,
            safety_failures: 0,
            reliability_lower: 0,
            latency_p95: 0,
            cost_mean: 0,
            input_tokens_mean: 0,
            output_tokens_mean: 0,
            attribution_coverage: 0,
        },
    )
}

fn variant_id(value: u8) -> VariantId {
    VariantId::new([value; 16]).expect("nonzero variant")
}

const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}
