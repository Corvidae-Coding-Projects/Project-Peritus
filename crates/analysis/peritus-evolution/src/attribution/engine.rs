//! Pure deterministic E3 metric projection and falsification.

use crate::{
    AttributionEntry, AttributionRecord, AttributionUnavailable, ChangeManifest, EvolutionError,
    EvolutionErrorKind, EvolutionLimits, EvolutionOperation, EvolutionRecovery,
    FalsificationVerdict, MetricObservation, MetricValue, Prediction, PredictionDirection,
    PredictionMetric, PredictionSubject, PublishedEvaluationEvidence, VariantDefinition,
};
use peritus_eval::TaskId;

/// Attributes every declared manifest prediction to one exact isolated variant or interaction group.
///
/// # Errors
/// Rejects manifest/variant/evaluation drift, duplicate/noncanonical manifests, bound excess, or
/// checked coverage/count overflow.
pub fn attribute(
    variant: &VariantDefinition,
    manifests: &[ChangeManifest],
    evaluation: &PublishedEvaluationEvidence,
    limits: EvolutionLimits,
) -> Result<AttributionRecord, EvolutionError> {
    if manifests.is_empty()
        || manifests.windows(2).any(|pair| pair[0].id() >= pair[1].id())
        || manifests.iter().map(ChangeManifest::id).collect::<Vec<_>>() != variant.manifest_ids()
        || evaluation.baseline().revision() != variant.baseline().revision()
        || evaluation.candidate().revision() != variant.candidate().revision()
        || evaluation.baseline().harness_revision() != variant.baseline().harness_revision()
        || evaluation.candidate().harness_revision() != variant.candidate().harness_revision()
    {
        return Err(binding());
    }
    let predicted = manifests.iter().map(|manifest| manifest.predictions().len()).sum::<usize>();
    if predicted == 0
        || predicted > usize::try_from(limits.attribution_entries()).unwrap_or(usize::MAX)
    {
        return Err(EvolutionError::new(
            EvolutionErrorKind::LimitExceeded,
            EvolutionOperation::Attribute,
            EvolutionRecovery::ReduceScope,
            "attribution entry population is empty or over limit",
        ));
    }
    let mut entries = Vec::with_capacity(predicted);
    for manifest in manifests {
        for prediction in manifest.predictions() {
            let observation = observe(prediction, evaluation);
            let verdict = verdict(prediction, observation);
            entries.push(AttributionEntry::new(
                manifest.id(),
                prediction.digest(),
                observation,
                verdict,
                prediction.mandatory(),
                prediction.critical(),
            ));
        }
    }
    AttributionRecord::from_exact_parts(
        variant.id(),
        evaluation.digest(),
        variant.interaction_group(),
        entries,
        limits,
    )
}

fn observe(prediction: &Prediction, evaluation: &PublishedEvaluationEvidence) -> MetricObservation {
    if matches!(prediction.subject(), PredictionSubject::FailureClass(_)) {
        return MetricObservation::Unavailable(AttributionUnavailable::UnsupportedFailureClass);
    }
    let analysis = evaluation.analysis();
    match prediction.metric() {
        PredictionMetric::CandidateCorrectnessLower => metric_observation(
            analysis.candidate_correctness_lower(),
            MetricValue::ProbabilityMillionths,
        ),
        PredictionMetric::PairedEffectLower => {
            metric_observation(analysis.paired_effect_lower(), MetricValue::SignedMillionths)
        }
        PredictionMetric::TaskPassAtK(k) => {
            let PredictionSubject::Task(task) = prediction.subject() else {
                return MetricObservation::Unavailable(AttributionUnavailable::TaskAbsent);
            };
            task_pass_at_k(analysis.candidate_pass_at_k(), task, k)
        }
        PredictionMetric::SafetyFailures => {
            MetricObservation::Available(MetricValue::Count(analysis.candidate_safety_failures()))
        }
        PredictionMetric::ReliabilityLower => {
            metric_observation(analysis.reliability_lower(), MetricValue::ProbabilityMillionths)
        }
        PredictionMetric::LatencyP95Micros => {
            metric_observation(analysis.latency_p95_micros(), MetricValue::Quantity)
        }
        PredictionMetric::CostMeanMicrounits => {
            metric_observation(analysis.cost_mean_microunits(), MetricValue::Quantity)
        }
        PredictionMetric::InputTokensMean => {
            metric_observation(analysis.input_tokens_mean(), MetricValue::Quantity)
        }
        PredictionMetric::OutputTokensMean => {
            metric_observation(analysis.output_tokens_mean(), MetricValue::Quantity)
        }
        PredictionMetric::TraceCompleteness => {
            completeness(analysis.complete_trace_rollouts(), analysis.expected_rollouts())
        }
        PredictionMetric::TeardownCompleteness => {
            completeness(analysis.complete_teardown_rollouts(), analysis.expected_rollouts())
        }
    }
}

fn metric_observation<T: Copy>(
    value: crate::EvaluationMetric<T>,
    project: impl FnOnce(T) -> MetricValue,
) -> MetricObservation {
    match value {
        crate::EvaluationMetric::Available(value) => MetricObservation::Available(project(value)),
        crate::EvaluationMetric::Unavailable(reason) => unavailable(reason),
    }
}

fn task_pass_at_k(
    value: &crate::EvaluationMetric<Vec<crate::TaskPassAtKSnapshot>>,
    task: TaskId,
    k: u16,
) -> MetricObservation {
    match value {
        crate::EvaluationMetric::Unavailable(reason) => unavailable(*reason),
        crate::EvaluationMetric::Available(tasks) => {
            if !tasks.iter().any(|value| value.task_id() == task) {
                return MetricObservation::Unavailable(AttributionUnavailable::TaskAbsent);
            }
            tasks.iter().find(|value| value.task_id() == task && value.k() == k).map_or(
                MetricObservation::Unavailable(AttributionUnavailable::MetricAbsent),
                |value| {
                    MetricObservation::Available(MetricValue::ProbabilityMillionths(
                        value.estimate_millionths(),
                    ))
                },
            )
        }
    }
}

fn completeness(complete: u32, expected: u32) -> MetricObservation {
    let value = u64::from(complete)
        .checked_mul(1_000_000)
        .and_then(|value| value.checked_div(u64::from(expected)));
    value
        .and_then(|value| u32::try_from(value).ok())
        .map_or(MetricObservation::Unavailable(AttributionUnavailable::Arithmetic), |value| {
            MetricObservation::Available(MetricValue::ProbabilityMillionths(value))
        })
}

const fn unavailable(reason: peritus_eval::MetricUnavailableReason) -> MetricObservation {
    MetricObservation::Unavailable(AttributionUnavailable::Evaluation(reason))
}

fn verdict(prediction: &Prediction, observation: MetricObservation) -> FalsificationVerdict {
    match observation {
        MetricObservation::Available(value) => {
            let confirmed = match prediction.direction() {
                PredictionDirection::AtLeast => value >= prediction.threshold(),
                PredictionDirection::AtMost => value <= prediction.threshold(),
                PredictionDirection::Equal => value == prediction.threshold(),
            };
            if confirmed {
                FalsificationVerdict::Confirmed
            } else {
                FalsificationVerdict::Contradicted
            }
        }
        MetricObservation::Unavailable(
            AttributionUnavailable::TaskAbsent | AttributionUnavailable::MetricAbsent,
        ) => FalsificationVerdict::NotObserved,
        MetricObservation::Unavailable(_) => FalsificationVerdict::Inconclusive,
    }
}

const fn binding() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::BindingDrift,
        EvolutionOperation::Attribute,
        EvolutionRecovery::CorrectInput,
        "variant, manifest, and evaluation bindings differ",
    )
}
