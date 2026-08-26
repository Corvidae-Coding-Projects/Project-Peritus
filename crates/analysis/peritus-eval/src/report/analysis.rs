//! Complete ledger-to-statistics analysis.

use std::collections::BTreeMap;

use crate::{
    ArmCorrectness, ArmResourceSummary, DistributionSummary, EvaluationArm, EvaluationError,
    EvaluationErrorKind, EvaluationOperation, EvaluationPlan, EvaluationReliability,
    FrozenEvaluationProfile, InfrastructureTreatment, LedgerCounts, MetricAvailability,
    MetricUnavailableReason, PairedCell, PairedEvidence, ResultDigest, RolloutLedger,
    RolloutOutcome, RolloutRecord, TaskFailureClass, TaskId, TaskPassAtK, TaskStability,
    WilsonInterval, analyze_stability, compare_paired, pass_at_k,
};

/// Complete deterministic E3 analysis, with no promotion authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationAnalysis {
    pub(super) baseline: ArmCorrectness,
    pub(super) candidate: ArmCorrectness,
    pub(super) paired: MetricAvailability<PairedEvidence>,
    pub(super) baseline_resources: ArmResourceSummary,
    pub(super) candidate_resources: ArmResourceSummary,
    pub(super) reliability: EvaluationReliability,
    pub(super) digest: ResultDigest,
}

impl EvaluationAnalysis {
    /// Baseline correctness evidence.
    #[must_use]
    pub const fn baseline(&self) -> &ArmCorrectness {
        &self.baseline
    }
    /// Candidate correctness evidence.
    #[must_use]
    pub const fn candidate(&self) -> &ArmCorrectness {
        &self.candidate
    }
    /// Paired comparison or exact unavailable reason.
    #[must_use]
    pub const fn paired(&self) -> &MetricAvailability<PairedEvidence> {
        &self.paired
    }
    /// Baseline resource evidence.
    #[must_use]
    pub const fn baseline_resources(&self) -> &ArmResourceSummary {
        &self.baseline_resources
    }
    /// Candidate resource evidence.
    #[must_use]
    pub const fn candidate_resources(&self) -> &ArmResourceSummary {
        &self.candidate_resources
    }
    /// Campaign reliability evidence.
    #[must_use]
    pub const fn reliability(&self) -> EvaluationReliability {
        self.reliability
    }
    /// Digest of every analysis value and policy identity.
    #[must_use]
    pub const fn digest(&self) -> ResultDigest {
        self.digest
    }
}

/// Analyzes only a complete ledger bound to the exact plan/profile.
///
/// # Errors
/// Rejects incomplete accounting, record drift, invalid metric denominators, or checked overflow.
#[allow(
    clippy::suspicious_operation_groupings,
    reason = "the binding deliberately compares record, profile, plan, and rollout accessors with different names"
)]
pub fn analyze_evaluation(
    plan: &EvaluationPlan,
    profile: &FrozenEvaluationProfile,
    ledger: &RolloutLedger,
) -> Result<EvaluationAnalysis, EvaluationError> {
    if plan.digest().as_bytes() == &[0; 32] || !ledger.complete() {
        return Err(crate::invalid(
            EvaluationErrorKind::Incomplete,
            EvaluationOperation::Analyze,
            "analysis requires a complete frozen rollout ledger",
        ));
    }
    let mut records = BTreeMap::new();
    for spec in plan.specs() {
        let record = ledger.record(spec.id()).ok_or_else(incomplete)?;
        if (record.profile_digest() != profile.digest())
            || (record.campaign_id() != plan.campaign_id())
            || (record.task_id() != spec.task_id())
            || (record.arm() != spec.arm())
            || (record.ordinal() != spec.ordinal())
            || (record.seed() != spec.seed())
            || (record.work_id() != spec.work_id())
            || (record.request_digest() != spec.request_digest())
        {
            return Err(binding());
        }
        records.insert(spec.id(), record);
    }

    let baseline_records = arm_records(plan, &records, EvaluationArm::Baseline);
    let candidate_records = arm_records(plan, &records, EvaluationArm::Candidate);
    let baseline = correctness(profile, &baseline_records)?;
    let candidate = correctness(profile, &candidate_records)?;
    let paired = paired(profile, plan, &records)?;
    let baseline_resources = resources(profile, &baseline_records)?;
    let candidate_resources = resources(profile, &candidate_records)?;
    let reliability = reliability(plan, ledger)?;
    let mut analysis = EvaluationAnalysis {
        baseline,
        candidate,
        paired,
        baseline_resources,
        candidate_resources,
        reliability,
        digest: ResultDigest::new(peritus_types::Sha256Digest::new([0; 32])),
    };
    analysis.digest = ResultDigest::new(peritus_codec::sha256(&super::canonical::analysis_bytes(
        profile.digest(),
        &analysis,
    )?));
    Ok(analysis)
}

fn arm_records(
    plan: &EvaluationPlan,
    records: &BTreeMap<crate::RolloutId, RolloutRecord>,
    arm: EvaluationArm,
) -> Vec<RolloutRecord> {
    plan.specs()
        .iter()
        .filter(|spec| spec.arm() == arm)
        .filter_map(|spec| records.get(&spec.id()).copied())
        .collect()
}

#[allow(
    clippy::too_many_lines,
    reason = "the closed correctness pipeline keeps denominator, interval, pass-at-k, and stability steps together"
)]
fn correctness(
    profile: &FrozenEvaluationProfile,
    records: &[RolloutRecord],
) -> Result<ArmCorrectness, EvaluationError> {
    let raw = counts(records)?;
    let safety_failures = u32::try_from(
        records
            .iter()
            .filter(|record| {
                matches!(
                    record.outcome(),
                    RolloutOutcome::TaskFailed { class: TaskFailureClass::Safety, .. }
                )
            })
            .count(),
    )
    .map_err(|_| arithmetic())?;
    let unavailable = correctness_unavailable(raw, profile.infrastructure().correctness());
    let excluded_infrastructure = if matches!(
        profile.infrastructure().correctness(),
        InfrastructureTreatment::ExcludeWithDenominator
    ) {
        raw.infrastructure_failed
    } else {
        0
    };
    let (interval, task_values) = if let Some(reason) = unavailable {
        (MetricAvailability::Unavailable(reason), MetricAvailability::Unavailable(reason))
    } else {
        let mut by_task: BTreeMap<TaskId, Vec<bool>> = BTreeMap::new();
        for record in records {
            if let Some(value) =
                correctness_value(record.outcome(), profile.infrastructure().correctness())
            {
                by_task.entry(record.task_id()).or_default().push(value);
            }
        }
        let successes = by_task.values().flatten().filter(|value| **value).count();
        let total = by_task.values().map(Vec::len).sum::<usize>();
        if total == 0 {
            (
                MetricAvailability::Unavailable(MetricUnavailableReason::EmptyDenominator),
                MetricAvailability::Unavailable(MetricUnavailableReason::EmptyDenominator),
            )
        } else {
            let success_u32 = u32::try_from(successes).map_err(|_| arithmetic())?;
            let total_u32 = u32::try_from(total).map_err(|_| arithmetic())?;
            let mut pass_values = Vec::with_capacity(by_task.len());
            for (task_id, outcomes) in &by_task {
                let n = u32::try_from(outcomes.len()).map_err(|_| arithmetic())?;
                let c = u32::try_from(outcomes.iter().filter(|value| **value).count())
                    .map_err(|_| arithmetic())?;
                let values = profile
                    .metrics()
                    .pass_k()
                    .iter()
                    .filter(|k| u32::from(**k) <= n)
                    .map(|k| pass_at_k(n, c, *k))
                    .collect::<Result<Vec<_>, _>>()?;
                pass_values.push(TaskPassAtK { task_id: *task_id, values });
            }
            (
                MetricAvailability::Available(WilsonInterval::ninety_five(success_u32, total_u32)?),
                MetricAvailability::Available(pass_values),
            )
        }
    };
    let stability = match task_values {
        MetricAvailability::Unavailable(reason) => MetricAvailability::Unavailable(reason),
        MetricAvailability::Available(_) => {
            let mut by_task: BTreeMap<TaskId, Vec<bool>> = BTreeMap::new();
            for record in records {
                if let Some(value) =
                    correctness_value(record.outcome(), profile.infrastructure().correctness())
                {
                    by_task.entry(record.task_id()).or_default().push(value);
                }
            }
            MetricAvailability::Available(
                by_task
                    .into_iter()
                    .map(|(task_id, outcomes)| {
                        Ok(TaskStability {
                            task_id,
                            summary: analyze_stability(
                                &outcomes,
                                profile.metrics().instability_threshold_millionths(),
                            )?,
                        })
                    })
                    .collect::<Result<Vec<_>, EvaluationError>>()?,
            )
        }
    };
    Ok(ArmCorrectness {
        raw,
        safety_failures,
        excluded_infrastructure,
        raw_success_interval: interval,
        pass_at_k: task_values,
        stability,
    })
}

fn paired(
    profile: &FrozenEvaluationProfile,
    plan: &EvaluationPlan,
    records: &BTreeMap<crate::RolloutId, RolloutRecord>,
) -> Result<MetricAvailability<PairedEvidence>, EvaluationError> {
    let mut pairs: BTreeMap<(TaskId, u16), [Option<RolloutRecord>; 2]> = BTreeMap::new();
    for spec in plan.specs() {
        let index = match spec.arm() {
            EvaluationArm::Baseline => 0,
            EvaluationArm::Candidate => 1,
        };
        pairs.entry((spec.task_id(), spec.ordinal())).or_insert([None, None])[index] =
            records.get(&spec.id()).copied();
    }
    let treatment = profile.infrastructure().correctness();
    let mut cells = Vec::new();
    let mut invalid_pairs = 0_u32;
    for ((task, ordinal), [baseline, candidate]) in pairs {
        let baseline = baseline.ok_or_else(incomplete)?;
        let candidate = candidate.ok_or_else(incomplete)?;
        for outcome in [baseline.outcome(), candidate.outcome()] {
            match outcome {
                RolloutOutcome::Cancelled => {
                    return Ok(MetricAvailability::Unavailable(
                        MetricUnavailableReason::CancelledRollout,
                    ));
                }
                RolloutOutcome::Ambiguous { .. } => {
                    return Ok(MetricAvailability::Unavailable(
                        MetricUnavailableReason::AmbiguousRollout,
                    ));
                }
                RolloutOutcome::InfrastructureFailed { .. }
                    if treatment == InfrastructureTreatment::InvalidateMetric =>
                {
                    return Ok(MetricAvailability::Unavailable(
                        MetricUnavailableReason::InfrastructureInvalidated,
                    ));
                }
                _ => {}
            }
        }
        match (
            correctness_value(baseline.outcome(), treatment),
            correctness_value(candidate.outcome(), treatment),
        ) {
            (Some(left), Some(right)) => cells.push(PairedCell::new(task, ordinal, left, right)?),
            _ => invalid_pairs = invalid_pairs.checked_add(1).ok_or_else(arithmetic)?,
        }
    }
    if cells.is_empty() {
        return Ok(MetricAvailability::Unavailable(MetricUnavailableReason::EmptyDenominator));
    }
    Ok(MetricAvailability::Available(PairedEvidence {
        comparison: compare_paired(
            profile.digest(),
            &cells,
            profile.metrics().bootstrap_replicates(),
            profile.metrics().confidence_millionths(),
        )?,
        invalid_pairs,
    }))
}

fn resources(
    profile: &FrozenEvaluationProfile,
    records: &[RolloutRecord],
) -> Result<ArmResourceSummary, EvaluationError> {
    let mut elapsed = Vec::new();
    let mut cost = Vec::new();
    let mut input = Vec::new();
    let mut output = Vec::new();
    let mut cpu = Vec::new();
    let mut memory = Vec::new();
    let mut missing_elapsed = 0_u32;
    let mut missing_cost = 0_u32;
    let mut missing_input = 0_u32;
    let mut missing_output = 0_u32;
    let mut missing_cpu = 0_u32;
    let mut missing_memory = 0_u32;
    for record in records {
        let stages = [record.candidate_resources(), record.evaluator_resources()];
        let present: Vec<_> = stages.into_iter().flatten().collect();
        if present.is_empty() {
            missing_elapsed += 1;
            missing_cost += 1;
            missing_input += 1;
            missing_output += 1;
            missing_cpu += 1;
            missing_memory += 1;
            continue;
        }
        elapsed.push(sum_required(&present, |value| Some(value.elapsed_micros()))?);
        collect_sum(&present, ResourceObservationView::Cost, &mut cost, &mut missing_cost)?;
        collect_sum(&present, ResourceObservationView::Input, &mut input, &mut missing_input)?;
        collect_sum(&present, ResourceObservationView::Output, &mut output, &mut missing_output)?;
        collect_sum(&present, ResourceObservationView::Cpu, &mut cpu, &mut missing_cpu)?;
        collect_max(&present, &mut memory, &mut missing_memory);
    }
    let required = profile.metrics().require_complete_usage();
    Ok(ArmResourceSummary {
        elapsed_micros: distribution(elapsed, missing_elapsed, false)?,
        cost_microunits: distribution(cost, missing_cost, required)?,
        input_tokens: distribution(input, missing_input, required)?,
        output_tokens: distribution(output, missing_output, required)?,
        cpu_micros: distribution(cpu, missing_cpu, false)?,
        memory_high_water_bytes: distribution(memory, missing_memory, false)?,
    })
}

#[derive(Clone, Copy)]
enum ResourceObservationView {
    Cost,
    Input,
    Output,
    Cpu,
}

fn collect_sum(
    values: &[crate::ResourceObservation],
    view: ResourceObservationView,
    output: &mut Vec<u64>,
    missing: &mut u32,
) -> Result<(), EvaluationError> {
    let value = sum_required(values, |value| match view {
        ResourceObservationView::Cost => value.cost_microunits(),
        ResourceObservationView::Input => value.input_tokens(),
        ResourceObservationView::Output => value.output_tokens(),
        ResourceObservationView::Cpu => value.cpu_micros(),
    });
    match value {
        Ok(value) => output.push(value),
        Err(error) if error.kind() == EvaluationErrorKind::Incomplete => *missing += 1,
        Err(error) => return Err(error),
    }
    Ok(())
}

fn sum_required(
    values: &[crate::ResourceObservation],
    read: impl Fn(crate::ResourceObservation) -> Option<u64>,
) -> Result<u64, EvaluationError> {
    values.iter().try_fold(0_u64, |total, value| {
        total.checked_add(read(*value).ok_or_else(missing_resource)?).ok_or_else(arithmetic)
    })
}

fn collect_max(values: &[crate::ResourceObservation], output: &mut Vec<u64>, missing: &mut u32) {
    if values.iter().any(|value| value.memory_high_water_bytes().is_none()) {
        *missing += 1;
    } else if let Some(value) =
        values.iter().filter_map(|value| value.memory_high_water_bytes()).max()
    {
        output.push(value);
    }
}

fn distribution(
    values: Vec<u64>,
    missing: u32,
    required: bool,
) -> Result<MetricAvailability<DistributionSummary>, EvaluationError> {
    if required && missing > 0 {
        Ok(MetricAvailability::Unavailable(MetricUnavailableReason::RequiredObservationMissing))
    } else if values.is_empty() {
        Ok(MetricAvailability::Unavailable(MetricUnavailableReason::EmptyDenominator))
    } else {
        Ok(MetricAvailability::Available(DistributionSummary::new(values, missing)?))
    }
}

fn reliability(
    plan: &EvaluationPlan,
    ledger: &RolloutLedger,
) -> Result<EvaluationReliability, EvaluationError> {
    let counts = ledger.counts();
    let mut attempts = 0_u32;
    let mut retried = 0_u32;
    let mut trace = 0_u32;
    let mut teardown = 0_u32;
    for spec in plan.specs() {
        let count = ledger.attempts(spec.id()).ok_or_else(incomplete)?.len();
        attempts = attempts
            .checked_add(u32::try_from(count).map_err(|_| arithmetic())?)
            .ok_or_else(arithmetic)?;
        if count > 1 {
            retried = retried.checked_add(1).ok_or_else(arithmetic)?;
        }
        if let Some(record) = ledger.record(spec.id()) {
            let stages = [record.candidate_resources(), record.evaluator_resources()];
            let present: Vec<_> = stages.into_iter().flatten().collect();
            if !present.is_empty() && present.iter().all(|value| value.trace_complete()) {
                trace += 1;
            }
            if !present.is_empty() && present.iter().all(|value| value.teardown_complete()) {
                teardown += 1;
            }
        }
    }
    let total = counts.expected;
    let evaluated = counts.passed.checked_add(counts.task_failed).ok_or_else(arithmetic)?;
    Ok(EvaluationReliability {
        counts,
        attempts,
        retried_rollouts: retried,
        complete_trace_rollouts: trace,
        complete_teardown_rollouts: teardown,
        evaluated_interval: MetricAvailability::Available(WilsonInterval::ninety_five(
            evaluated, total,
        )?),
        infrastructure_interval: MetricAvailability::Available(WilsonInterval::ninety_five(
            counts.infrastructure_failed,
            total,
        )?),
    })
}

fn correctness_unavailable(
    counts: LedgerCounts,
    treatment: InfrastructureTreatment,
) -> Option<MetricUnavailableReason> {
    if counts.cancelled > 0 {
        Some(MetricUnavailableReason::CancelledRollout)
    } else if counts.ambiguous > 0 {
        Some(MetricUnavailableReason::AmbiguousRollout)
    } else if counts.infrastructure_failed > 0
        && treatment == InfrastructureTreatment::InvalidateMetric
    {
        Some(MetricUnavailableReason::InfrastructureInvalidated)
    } else {
        None
    }
}

const fn correctness_value(
    outcome: RolloutOutcome,
    treatment: InfrastructureTreatment,
) -> Option<bool> {
    match outcome {
        RolloutOutcome::TaskPassed { .. } => Some(true),
        RolloutOutcome::TaskFailed { .. } => Some(false),
        RolloutOutcome::InfrastructureFailed { .. } => match treatment {
            InfrastructureTreatment::CountAsFailure => Some(false),
            InfrastructureTreatment::ExcludeWithDenominator
            | InfrastructureTreatment::InvalidateMetric => None,
        },
        RolloutOutcome::Cancelled | RolloutOutcome::Ambiguous { .. } => None,
    }
}

fn counts(records: &[RolloutRecord]) -> Result<LedgerCounts, EvaluationError> {
    let mut counts = LedgerCounts {
        expected: u32::try_from(records.len()).map_err(|_| arithmetic())?,
        ..LedgerCounts::default()
    };
    for record in records {
        let value = match record.outcome() {
            RolloutOutcome::TaskPassed { .. } => &mut counts.passed,
            RolloutOutcome::TaskFailed { .. } => &mut counts.task_failed,
            RolloutOutcome::InfrastructureFailed { .. } => &mut counts.infrastructure_failed,
            RolloutOutcome::Cancelled => &mut counts.cancelled,
            RolloutOutcome::Ambiguous { .. } => &mut counts.ambiguous,
        };
        *value = value.checked_add(1).ok_or_else(arithmetic)?;
    }
    Ok(counts)
}

const fn incomplete() -> EvaluationError {
    crate::invalid(
        EvaluationErrorKind::Incomplete,
        EvaluationOperation::Analyze,
        "complete ledger is missing a planned rollout record",
    )
}
const fn binding() -> EvaluationError {
    crate::invalid(
        EvaluationErrorKind::Binding,
        EvaluationOperation::Analyze,
        "rollout record differs from its frozen plan binding",
    )
}
const fn arithmetic() -> EvaluationError {
    crate::invalid(
        EvaluationErrorKind::Statistics,
        EvaluationOperation::Analyze,
        "evaluation analysis checked arithmetic overflowed",
    )
}
const fn missing_resource() -> EvaluationError {
    crate::invalid(
        EvaluationErrorKind::Incomplete,
        EvaluationOperation::Analyze,
        "resource observation is explicitly missing",
    )
}
