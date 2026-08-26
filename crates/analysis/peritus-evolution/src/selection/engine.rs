//! Pure deny-wins assessment and stable lexicographic selection.

use core::cmp::Ordering;

use crate::{
    AttributionRecord, CompatibilityEffect, Criterion, CriterionOutcome, CriterionResult,
    EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery, MetricValue,
    Objective, ObjectiveVector, PromotionPolicy, PromotionReviewEvidence,
    PublishedEvaluationEvidence, SelectionDecision, SelectionRecord, VariantAssessment,
    VariantDefinition, VariantRejection,
};
use peritus_types::Sha256Digest;

/// Evaluates every independent mandatory promotion criterion for one exact variant.
///
/// # Errors
/// Rejects attribution/evaluation/variant drift. Missing metrics are retained as unavailable
/// criterion results and do not produce an error.
#[allow(
    clippy::too_many_lines,
    reason = "the closed fourteen-criterion policy table remains contiguous and auditable"
)]
pub fn assess_variant(
    variant: &VariantDefinition,
    attribution: &AttributionRecord,
    evaluation: &PublishedEvaluationEvidence,
    review: Option<PromotionReviewEvidence>,
    policy: &PromotionPolicy,
) -> Result<VariantAssessment, EvolutionError> {
    if attribution.variant_id() != variant.id()
        || attribution.evaluation_digest() != evaluation.digest()
        || review.is_some_and(|value| {
            value.candidate_revision_digest()
                != variant.candidate().harness_revision().digest().digest()
        })
    {
        return Err(binding("assessment inputs name different variant evidence"));
    }
    let thresholds = policy.thresholds();
    let analysis = evaluation.analysis();
    let paired = analysis.paired_effect_lower().value();
    let reliability = analysis.reliability_lower().value();
    let latency = analysis.latency_p95_micros().value();
    let cost = analysis.cost_mean_microunits().value();
    let input = analysis.input_tokens_mean().value();
    let output = analysis.output_tokens_mean().value();
    let review_required = requires_review(
        variant.changes_executable(),
        variant.changed_kinds(),
        policy.review_required_kinds(),
    );
    let mut criteria = vec![
        metric_result(
            Criterion::PairedCorrectness,
            paired.map(MetricValue::SignedMillionths),
            paired.is_some_and(|value| value >= thresholds.minimum_paired_lower_millionths()),
            evaluation.digest(),
        ),
        count_result(
            Criterion::CriticalRegressions,
            attribution.critical_regressions(),
            attribution.critical_regressions() <= thresholds.maximum_critical_regressions(),
            attribution.digest(),
        ),
        count_result(
            Criterion::Safety,
            analysis.candidate_safety_failures(),
            analysis.candidate_safety_failures() <= thresholds.maximum_safety_failures(),
            evaluation.digest(),
        ),
        metric_result(
            Criterion::Reliability,
            reliability.map(MetricValue::ProbabilityMillionths),
            reliability
                .is_some_and(|value| value >= thresholds.minimum_reliability_lower_millionths()),
            evaluation.digest(),
        ),
        metric_result(
            Criterion::AttributionCoverage,
            Some(MetricValue::ProbabilityMillionths(attribution.coverage_millionths())),
            attribution.coverage_millionths()
                >= thresholds.minimum_attribution_coverage_millionths(),
            attribution.digest(),
        ),
        count_result(
            Criterion::MandatoryPredictions,
            attribution.mandatory_failures(),
            attribution.mandatory_failures() == 0,
            attribution.digest(),
        ),
        metric_result(
            Criterion::Latency,
            latency.map(MetricValue::Quantity),
            latency.is_some_and(|value| value <= thresholds.maximum_latency_p95_micros()),
            evaluation.digest(),
        ),
        metric_result(
            Criterion::Cost,
            cost.map(MetricValue::Quantity),
            cost.is_some_and(|value| value <= thresholds.maximum_cost_mean_microunits()),
            evaluation.digest(),
        ),
        metric_result(
            Criterion::InputTokens,
            input.map(MetricValue::Quantity),
            input.is_some_and(|value| value <= thresholds.maximum_input_tokens_mean()),
            evaluation.digest(),
        ),
        metric_result(
            Criterion::OutputTokens,
            output.map(MetricValue::Quantity),
            output.is_some_and(|value| value <= thresholds.maximum_output_tokens_mean()),
            evaluation.digest(),
        ),
        completeness_result(
            Criterion::TraceCompleteness,
            thresholds.require_complete_trace(),
            analysis.complete_trace_rollouts(),
            analysis.expected_rollouts(),
            evaluation.digest(),
        ),
        completeness_result(
            Criterion::TeardownCompleteness,
            thresholds.require_complete_teardown(),
            analysis.complete_teardown_rollouts(),
            analysis.expected_rollouts(),
            evaluation.digest(),
        ),
        CriterionResult::new(
            Criterion::IndependentReview,
            if !review_required || review.is_some() {
                CriterionOutcome::Passed
            } else {
                CriterionOutcome::Unavailable
            },
            None,
            review.map_or_else(|| variant.digest(), PromotionReviewEvidence::digest),
        ),
        CriterionResult::new(
            Criterion::Compatibility,
            if variant.compatibility() == CompatibilityEffect::Incompatible
                || (!policy.allow_cross_lineage()
                    && variant.baseline().harness_revision().harness_id()
                        != variant.candidate().harness_revision().harness_id())
            {
                CriterionOutcome::Failed
            } else {
                CriterionOutcome::Passed
            },
            None,
            variant.digest(),
        ),
    ];
    criteria.sort_unstable_by_key(|result| result.criterion());
    let objectives = ObjectiveVector {
        paired_lower: paired.unwrap_or(i32::MIN),
        critical_regressions: attribution.critical_regressions(),
        safety_failures: analysis.candidate_safety_failures(),
        reliability_lower: reliability.unwrap_or(0),
        latency_p95: latency.unwrap_or(u64::MAX),
        cost_mean: cost.unwrap_or(u64::MAX),
        input_tokens_mean: input.unwrap_or(u64::MAX),
        output_tokens_mean: output.unwrap_or(u64::MAX),
        attribution_coverage: attribution.coverage_millionths(),
    };
    Ok(VariantAssessment::from_exact_parts(
        variant.id(),
        attribution.id(),
        evaluation.digest(),
        policy.digest(),
        criteria,
        objectives,
    ))
}

/// Selects one eligible variant by frozen objective order and stable variant identity.
///
/// # Errors
/// Rejects empty, duplicate/noncanonical, or policy-drifted assessments.
pub fn select_variant(
    policy: &PromotionPolicy,
    assessments: &[VariantAssessment],
) -> Result<SelectionRecord, EvolutionError> {
    if assessments.iter().any(|assessment| assessment.policy_digest() != policy.digest()) {
        return Err(binding("selection assessment belongs to another promotion policy"));
    }
    if assessments.is_empty()
        || assessments.windows(2).any(|pair| pair[0].variant_id() >= pair[1].variant_id())
        || assessments.len() > usize::from(policy.maximum_variants())
    {
        return Err(EvolutionError::new(
            EvolutionErrorKind::NonCanonical,
            EvolutionOperation::Select,
            EvolutionRecovery::CorrectInput,
            "selection assessments are empty, noncanonical, or over limit",
        ));
    }
    let selected = assessments
        .iter()
        .filter(|assessment| assessment.eligible())
        .min_by(|left, right| compare(left, right, policy.objectives()));
    let decision = selected.map_or_else(
        || {
            SelectionDecision::NoEligibleVariant(
                assessments
                    .iter()
                    .map(|assessment| {
                        let failed = assessment
                            .criteria()
                            .iter()
                            .filter(|value| value.outcome() == CriterionOutcome::Failed)
                            .map(|value| value.criterion())
                            .collect();
                        let unavailable = assessment
                            .criteria()
                            .iter()
                            .filter(|value| value.outcome() == CriterionOutcome::Unavailable)
                            .map(|value| value.criterion())
                            .collect();
                        VariantRejection::new(assessment.variant_id(), failed, unavailable)
                    })
                    .collect(),
            )
        },
        |assessment| SelectionDecision::Selected(assessment.variant_id()),
    );
    let assessment_digests = assessments.iter().map(VariantAssessment::digest).collect::<Vec<_>>();
    Ok(SelectionRecord::from_exact_parts(policy.digest(), assessment_digests, decision))
}

pub(super) fn requires_review(
    changes_executable: bool,
    changed_kinds: &[peritus_harness::domain::ComponentKind],
    policy_required_kinds: &[peritus_harness::domain::ComponentKind],
) -> bool {
    changes_executable
        || changed_kinds.iter().any(|kind| policy_required_kinds.binary_search(kind).is_ok())
}

const fn metric_result(
    criterion: Criterion,
    observed: Option<MetricValue>,
    passed: bool,
    evidence: Sha256Digest,
) -> CriterionResult {
    CriterionResult::new(
        criterion,
        if observed.is_none() {
            CriterionOutcome::Unavailable
        } else if passed {
            CriterionOutcome::Passed
        } else {
            CriterionOutcome::Failed
        },
        observed,
        evidence,
    )
}

const fn count_result(
    criterion: Criterion,
    observed: u32,
    passed: bool,
    evidence: Sha256Digest,
) -> CriterionResult {
    metric_result(criterion, Some(MetricValue::Count(observed)), passed, evidence)
}

fn completeness_result(
    criterion: Criterion,
    required: bool,
    complete: u32,
    expected: u32,
    evidence: Sha256Digest,
) -> CriterionResult {
    if !required {
        return CriterionResult::new(criterion, CriterionOutcome::Passed, None, evidence);
    }
    let observed = u64::from(complete)
        .checked_mul(1_000_000)
        .and_then(|value| value.checked_div(u64::from(expected)))
        .and_then(|value| u32::try_from(value).ok())
        .map(MetricValue::ProbabilityMillionths);
    metric_result(criterion, observed, complete == expected && expected > 0, evidence)
}

fn compare(
    left: &VariantAssessment,
    right: &VariantAssessment,
    objectives: &[Objective],
) -> Ordering {
    let left_values = left.objectives();
    let right_values = right.objectives();
    for objective in objectives {
        let ordering = match objective {
            Objective::PairedCorrectness => {
                right_values.paired_lower.cmp(&left_values.paired_lower)
            }
            Objective::CriticalRegressions => {
                left_values.critical_regressions.cmp(&right_values.critical_regressions)
            }
            Objective::SafetyFailures => {
                left_values.safety_failures.cmp(&right_values.safety_failures)
            }
            Objective::Reliability => {
                right_values.reliability_lower.cmp(&left_values.reliability_lower)
            }
            Objective::Latency => left_values.latency_p95.cmp(&right_values.latency_p95),
            Objective::Cost => left_values.cost_mean.cmp(&right_values.cost_mean),
            Objective::InputTokens => {
                left_values.input_tokens_mean.cmp(&right_values.input_tokens_mean)
            }
            Objective::OutputTokens => {
                left_values.output_tokens_mean.cmp(&right_values.output_tokens_mean)
            }
            Objective::AttributionCoverage => {
                right_values.attribution_coverage.cmp(&left_values.attribution_coverage)
            }
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.variant_id().cmp(&right.variant_id())
}

const fn binding(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::BindingDrift,
        EvolutionOperation::Select,
        EvolutionRecovery::CorrectInput,
        detail,
    )
}
