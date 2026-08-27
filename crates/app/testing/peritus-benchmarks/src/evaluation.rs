//! Deterministic percentile, SLO, regression, coverage, and readiness evaluation.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    AccountingSummary, BaselineManifest, MeasurementSet, Metric, MetricDirection, MetricSummary,
    NotReadyReason, ObjectiveEvaluation, ObjectiveStatus, QualificationError,
    QualificationEvaluation, QualificationProfile, QualificationVerdict, RegressionClass,
    RegressionResult, RunnerReceipt, ScenarioKind, StableId, Workload,
};

/// Stateless deterministic qualification evaluator.
pub struct QualificationEvaluator;

impl QualificationEvaluator {
    /// Evaluates coverage, runner completion, resources, SLOs, and baseline regression policy.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when run/profile bindings disagree, receipts or workloads
    /// are duplicated or invalid, workload reservations exceed the profile, or checked statistics
    /// overflow their integer representation.
    pub fn evaluate(
        profile: &QualificationProfile,
        workloads: &[Workload],
        measurements: &MeasurementSet,
        accounting: AccountingSummary,
        runner_receipts: &[RunnerReceipt],
        baseline: Option<&BaselineManifest>,
    ) -> Result<QualificationEvaluation, QualificationError> {
        require_binding("profile_id", profile.id(), measurements.profile_id())?;
        validate_receipts(measurements.run_id(), runner_receipts)?;
        let workload_map = validate_workloads(profile, workloads)?;
        let summaries = summarize(measurements)?;
        let summary_map = summaries
            .iter()
            .map(|summary| ((summary.workload_id().clone(), summary.metric()), summary))
            .collect::<BTreeMap<_, _>>();
        let observed_workloads = measurements.observed_workloads();
        let mut reasons = Vec::new();
        check_workload_coverage(
            profile,
            &workload_map,
            &observed_workloads,
            runner_receipts,
            &accounting,
            &mut reasons,
        );
        if !accounting.is_balanced() {
            reasons.push(NotReadyReason::UnbalancedResources);
        }

        let mut objectives = Vec::with_capacity(profile.objectives().len());
        let mut regressions = Vec::with_capacity(profile.objectives().len());
        for objective in profile.objectives() {
            let summary = summary_map.get(&(objective.workload_id().clone(), objective.metric()));
            let sample_count = summary.map_or(0, |value| value.sample_count());
            let observed = summary
                .filter(|value| value.sample_count() >= objective.minimum_samples())
                .map(|value| value.value(objective.statistic()));
            let status = match observed {
                Some(value) if objective.accepts(value) => ObjectiveStatus::Met,
                Some(_) => ObjectiveStatus::Missed,
                None => ObjectiveStatus::InsufficientEvidence,
            };
            match status {
                ObjectiveStatus::Met => {}
                ObjectiveStatus::Missed => reasons
                    .push(NotReadyReason::ObjectiveMissed { objective_id: objective.id().clone() }),
                ObjectiveStatus::InsufficientEvidence => {
                    reasons.push(NotReadyReason::InsufficientObjectiveEvidence {
                        objective_id: objective.id().clone(),
                    });
                }
            }
            objectives.push(ObjectiveEvaluation {
                objective_id: objective.id().clone(),
                workload_id: objective.workload_id().clone(),
                metric: objective.metric(),
                statistic: objective.statistic(),
                bound: objective.bound(),
                threshold: objective.threshold(),
                observed,
                sample_count,
                status,
            });
            let regression = compare(objective, observed, profile, baseline);
            if regression.class() == RegressionClass::Incomparable
                && profile.regression_policy().baseline_required()
            {
                reasons.push(NotReadyReason::RequiredBaselineMissing {
                    objective_id: objective.id().clone(),
                });
            }
            if regression.class() == RegressionClass::Blocking {
                reasons.push(NotReadyReason::BlockingRegression {
                    objective_id: objective.id().clone(),
                });
            }
            regressions.push(regression);
        }
        let verdict = if reasons.is_empty() {
            QualificationVerdict::Ready
        } else {
            QualificationVerdict::NotReady
        };
        Ok(QualificationEvaluation {
            profile_id: profile.id().clone(),
            run_id: measurements.run_id().clone(),
            summaries,
            objectives,
            regressions,
            accounting,
            runner_receipts: runner_receipts.to_vec(),
            verdict,
            not_ready_reasons: reasons,
        })
    }
}

fn summarize(measurements: &MeasurementSet) -> Result<Vec<MetricSummary>, QualificationError> {
    let mut grouped = BTreeMap::<(StableId, Metric), Vec<u64>>::new();
    for record in measurements.records() {
        grouped
            .entry((record.workload_id().clone(), record.metric()))
            .or_default()
            .push(record.value());
    }
    grouped
        .into_iter()
        .map(|((workload, metric), values)| MetricSummary::from_values(workload, metric, values))
        .collect()
}

fn validate_workloads<'a>(
    profile: &QualificationProfile,
    workloads: &'a [Workload],
) -> Result<BTreeMap<StableId, &'a Workload>, QualificationError> {
    let mut map = BTreeMap::new();
    for workload in workloads {
        workload.validate_against(profile.envelope())?;
        if map.insert(workload.id().clone(), workload).is_some() {
            return Err(QualificationError::Duplicate {
                kind: "workload",
                id: workload.id().to_string(),
            });
        }
    }
    Ok(map)
}

fn validate_receipts(
    run_id: &StableId,
    runner_receipts: &[RunnerReceipt],
) -> Result<(), QualificationError> {
    let mut workloads = BTreeSet::new();
    for receipt in runner_receipts {
        require_binding("runner_receipt.run_id", run_id, receipt.run_id())?;
        if !workloads.insert(receipt.workload_id()) {
            return Err(QualificationError::Duplicate {
                kind: "runner receipt workload",
                id: receipt.workload_id().to_string(),
            });
        }
    }
    Ok(())
}

fn check_workload_coverage(
    profile: &QualificationProfile,
    workloads: &BTreeMap<StableId, &Workload>,
    observed_workloads: &BTreeSet<&StableId>,
    receipts: &[RunnerReceipt],
    accounting: &AccountingSummary,
    reasons: &mut Vec<NotReadyReason>,
) {
    for workload_id in profile.required_workloads() {
        let Some(workload) = workloads.get(workload_id) else {
            reasons.push(NotReadyReason::MissingWorkloadDefinition {
                workload_id: workload_id.clone(),
            });
            continue;
        };
        match receipts.iter().find(|receipt| receipt.workload_id() == workload_id) {
            None => reasons
                .push(NotReadyReason::MissingRunnerReceipt { workload_id: workload_id.clone() }),
            Some(receipt)
                if receipt.expected_steps() != workload.parameters().operation_count() =>
            {
                reasons.push(NotReadyReason::RunnerPlanMismatch {
                    workload_id: workload_id.clone(),
                    expected: workload.parameters().operation_count(),
                    observed: receipt.expected_steps(),
                });
            }
            Some(receipt) if !receipt.completed() => {
                reasons.push(NotReadyReason::RunnerIncomplete {
                    workload_id: workload_id.clone(),
                    termination: receipt.termination(),
                });
            }
            Some(_) => {}
        }
        if !observed_workloads.contains(workload_id) {
            reasons.push(NotReadyReason::MissingMeasurements { workload_id: workload_id.clone() });
        }
        check_resource_exercise(workload, accounting, reasons);
    }
}

fn check_resource_exercise(
    workload: &Workload,
    accounting: &AccountingSummary,
    reasons: &mut Vec<NotReadyReason>,
) {
    let target = u64::from(workload.parameters().max_concurrency());
    let (resource, observed) = match workload.scenario() {
        ScenarioKind::ConcurrentRuns | ScenarioKind::MemoryBounds => {
            ("active_runs", u64::from(accounting.peak_runs()))
        }
        ScenarioKind::TerminalStreaming | ScenarioKind::Cancellation => {
            ("active_processes", u64::from(accounting.peak_processes()))
        }
        ScenarioKind::TokenFlow => {
            ("provider_requests", u64::from(accounting.peak_provider_requests()))
        }
        ScenarioKind::ProviderBackpressure => {
            ("provider_queue_depth", u64::from(accounting.peak_queue(crate::QueueKind::Provider)))
        }
        ScenarioKind::QueueSaturation => {
            ("command_queue_depth", u64::from(accounting.peak_queue(crate::QueueKind::Command)))
        }
        ScenarioKind::ExporterBackpressure => {
            ("exporter_queue_depth", u64::from(accounting.peak_queue(crate::QueueKind::Exporter)))
        }
        ScenarioKind::EventAppend | ScenarioKind::Recovery | ScenarioKind::DiskArtifacts => return,
    };
    let expected = match workload.scenario() {
        ScenarioKind::QueueSaturation
        | ScenarioKind::ExporterBackpressure
        | ScenarioKind::ProviderBackpressure => u64::from(workload.parameters().queue_capacity()),
        _ => target,
    };
    if observed < expected {
        reasons.push(NotReadyReason::ResourceExerciseMissing {
            workload_id: workload.id().clone(),
            resource,
            expected,
            observed,
        });
    }
}

fn compare(
    objective: &crate::SloObjective,
    candidate: Option<u64>,
    profile: &QualificationProfile,
    baseline: Option<&BaselineManifest>,
) -> RegressionResult {
    let entry =
        baseline.filter(|manifest| manifest.profile_id() == profile.id()).and_then(|manifest| {
            manifest.find(objective.workload_id(), objective.metric(), objective.statistic())
        });
    let (Some(candidate), Some(entry)) = (candidate, entry) else {
        return RegressionResult::new(
            objective.id().clone(),
            RegressionClass::Incomparable,
            entry.map(crate::BaselineEntry::value),
            candidate,
            None,
            None,
        );
    };
    let baseline_value = entry.value();
    let delta = candidate.abs_diff(baseline_value);
    let relative = if baseline_value == 0 {
        if delta == 0 { 0 } else { u64::MAX }
    } else {
        let scaled = u128::from(delta) * 10_000;
        u64::try_from(scaled / u128::from(baseline_value)).unwrap_or(u64::MAX)
    };
    let policy = profile.regression_policy();
    let improved = match objective.metric().direction() {
        MetricDirection::LowerIsBetter => candidate < baseline_value,
        MetricDirection::HigherIsBetter => candidate > baseline_value,
    };
    let class = if delta < policy.minimum_absolute_delta() {
        RegressionClass::Stable
    } else if improved {
        RegressionClass::Improvement
    } else if relative >= u64::from(policy.blocking_basis_points()) {
        RegressionClass::Blocking
    } else if relative >= u64::from(policy.warning_basis_points()) {
        RegressionClass::Warning
    } else {
        RegressionClass::Stable
    };
    RegressionResult::new(
        objective.id().clone(),
        class,
        Some(baseline_value),
        Some(candidate),
        Some(delta),
        Some(relative),
    )
}

fn require_binding(
    field: &'static str,
    expected: &StableId,
    observed: &StableId,
) -> Result<(), QualificationError> {
    if expected == observed {
        Ok(())
    } else {
        Err(QualificationError::MeasurementBinding {
            field,
            expected: expected.to_string(),
            observed: observed.to_string(),
        })
    }
}
