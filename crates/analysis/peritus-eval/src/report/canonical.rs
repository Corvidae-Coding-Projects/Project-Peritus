//! Canonical analysis and report encoding.

use peritus_codec::{CanonicalWriter, CodecLimits};

use crate::{
    ArmCorrectness, ArmResourceSummary, DistributionSummary, EvaluationAnalysis, EvaluationError,
    EvaluationErrorKind, EvaluationOperation, EvaluationRecovery, LedgerCounts, MetricAvailability,
    PairedEvidence, TaskPassAtK, TaskStability, WilsonInterval,
};

const ANALYSIS_DOMAIN: &[u8] = b"peritus.evaluation.analysis.v1\0";
const REPORT_DOMAIN: &[u8] = b"peritus.evaluation.report.v1\0";

pub(super) fn analysis_bytes(
    profile: crate::ProfileDigest,
    analysis: &EvaluationAnalysis,
) -> Result<Vec<u8>, EvaluationError> {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    writer.write_bytes(ANALYSIS_DOMAIN).map_err(codec)?;
    writer.write_fixed(profile.as_bytes()).map_err(codec)?;
    correctness(&mut writer, &analysis.baseline)?;
    correctness(&mut writer, &analysis.candidate)?;
    paired(&mut writer, &analysis.paired)?;
    resources(&mut writer, &analysis.baseline_resources)?;
    resources(&mut writer, &analysis.candidate_resources)?;
    reliability(&mut writer, analysis.reliability)?;
    Ok(writer.into_bytes())
}

pub(super) fn report_bytes(
    report: &super::validation::EvaluationReport,
) -> Result<Vec<u8>, EvaluationError> {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    writer.write_bytes(REPORT_DOMAIN).map_err(codec)?;
    writer.write_fixed(report.id.as_bytes()).map_err(codec)?;
    writer.write_fixed(report.campaign_id.as_bytes()).map_err(codec)?;
    writer.write_fixed(report.dataset_digest.as_bytes()).map_err(codec)?;
    writer.write_fixed(report.profile_digest.as_bytes()).map_err(codec)?;
    writer.write_fixed(report.plan_digest.as_bytes()).map_err(codec)?;
    writer.write_fixed(report.analysis.digest().as_bytes()).map_err(codec)?;
    writer.write_option_tag(report.supersedes.is_some()).map_err(codec)?;
    if let Some(value) = report.supersedes {
        writer.write_fixed(value.as_bytes()).map_err(codec)?;
    }
    writer.write_collection_len(report.violations.len()).map_err(codec)?;
    for violation in &report.violations {
        writer.write_u8(violation.kind.tag()).map_err(codec)?;
        writer.write_u32(violation.affected).map_err(codec)?;
        writer.write_fixed(violation.evidence_digest.as_bytes()).map_err(codec)?;
    }
    let analysis = analysis_bytes(report.profile_digest, &report.analysis)?;
    writer.write_bytes(&analysis).map_err(codec)?;
    Ok(writer.into_bytes())
}

fn correctness(
    writer: &mut CanonicalWriter,
    value: &ArmCorrectness,
) -> Result<(), EvaluationError> {
    counts(writer, value.raw)?;
    writer.write_u32(value.safety_failures).map_err(codec)?;
    writer.write_u32(value.excluded_infrastructure).map_err(codec)?;
    interval(writer, &value.raw_success_interval)?;
    pass_at_k(writer, &value.pass_at_k)?;
    stability(writer, &value.stability)
}

fn counts(writer: &mut CanonicalWriter, value: LedgerCounts) -> Result<(), EvaluationError> {
    for item in [
        value.expected,
        value.passed,
        value.task_failed,
        value.infrastructure_failed,
        value.cancelled,
        value.ambiguous,
    ] {
        writer.write_u32(item).map_err(codec)?;
    }
    Ok(())
}

fn unavailable<T>(
    writer: &mut CanonicalWriter,
    value: &MetricAvailability<T>,
) -> Result<bool, EvaluationError> {
    match value {
        MetricAvailability::Available(_) => {
            writer.write_u8(1).map_err(codec)?;
            Ok(false)
        }
        MetricAvailability::Unavailable(reason) => {
            writer.write_u8(2).map_err(codec)?;
            writer.write_u8(reason.tag()).map_err(codec)?;
            Ok(true)
        }
    }
}

fn interval(
    writer: &mut CanonicalWriter,
    value: &MetricAvailability<WilsonInterval>,
) -> Result<(), EvaluationError> {
    if unavailable(writer, value)? {
        return Ok(());
    }
    let value = *value.value().ok_or_else(corruption)?;
    writer.write_u32(value.successes()).map_err(codec)?;
    writer.write_u32(value.total()).map_err(codec)?;
    writer.write_u32(value.confidence_millionths()).map_err(codec)?;
    writer.write_u32(value.lower().get()).map_err(codec)?;
    writer.write_u32(value.upper().get()).map_err(codec)
}

fn pass_at_k(
    writer: &mut CanonicalWriter,
    value: &MetricAvailability<Vec<TaskPassAtK>>,
) -> Result<(), EvaluationError> {
    if unavailable(writer, value)? {
        return Ok(());
    }
    let values = value.value().ok_or_else(corruption)?;
    writer.write_collection_len(values.len()).map_err(codec)?;
    for task in values {
        writer.write_fixed(task.task_id().as_bytes()).map_err(codec)?;
        writer.write_collection_len(task.values().len()).map_err(codec)?;
        for metric in task.values() {
            writer.write_u32(metric.total()).map_err(codec)?;
            writer.write_u32(metric.successes()).map_err(codec)?;
            writer.write_u16(metric.k()).map_err(codec)?;
            writer.write_u32(metric.estimate().get()).map_err(codec)?;
        }
    }
    Ok(())
}

fn stability(
    writer: &mut CanonicalWriter,
    value: &MetricAvailability<Vec<TaskStability>>,
) -> Result<(), EvaluationError> {
    if unavailable(writer, value)? {
        return Ok(());
    }
    let values = value.value().ok_or_else(corruption)?;
    writer.write_collection_len(values.len()).map_err(codec)?;
    for task in values {
        let summary = task.summary();
        writer.write_fixed(task.task_id().as_bytes()).map_err(codec)?;
        writer.write_u32(summary.passes()).map_err(codec)?;
        writer.write_u32(summary.failures()).map_err(codec)?;
        writer.write_u32(summary.transitions()).map_err(codec)?;
        writer.write_u32(summary.longest_pass_streak()).map_err(codec)?;
        writer.write_u32(summary.longest_failure_streak()).map_err(codec)?;
        writer.write_u32(summary.agreement().get()).map_err(codec)?;
        writer
            .write_u8(match summary.class() {
                crate::StabilityClass::AlwaysPass => 1,
                crate::StabilityClass::AlwaysFail => 2,
                crate::StabilityClass::Unstable => 3,
                crate::StabilityClass::Mixed => 4,
            })
            .map_err(codec)?;
    }
    Ok(())
}

fn paired(
    writer: &mut CanonicalWriter,
    value: &MetricAvailability<PairedEvidence>,
) -> Result<(), EvaluationError> {
    if unavailable(writer, value)? {
        return Ok(());
    }
    let value = *value.value().ok_or_else(corruption)?;
    let comparison = value.comparison();
    let table = comparison.table();
    for item in [
        table.both_passed,
        table.candidate_only,
        table.baseline_only,
        table.both_failed,
        value.invalid_pairs(),
    ] {
        writer.write_u32(item).map_err(codec)?;
    }
    writer
        .write_fixed(&i64::from(comparison.net_effect_millionths()).to_be_bytes())
        .map_err(codec)?;
    let interval = comparison.interval();
    writer.write_fixed(&i64::from(interval.lower_millionths()).to_be_bytes()).map_err(codec)?;
    writer.write_fixed(&i64::from(interval.upper_millionths()).to_be_bytes()).map_err(codec)?;
    writer.write_u32(interval.replicates()).map_err(codec)?;
    writer.write_u32(interval.confidence_millionths()).map_err(codec)?;
    let sign = comparison.sign_test();
    writer.write_u32(sign.positive_tasks()).map_err(codec)?;
    writer.write_u32(sign.negative_tasks()).map_err(codec)?;
    writer.write_u32(sign.tied_tasks()).map_err(codec)?;
    writer.write_u32(sign.two_sided_p().get()).map_err(codec)
}

fn resources(
    writer: &mut CanonicalWriter,
    value: &ArmResourceSummary,
) -> Result<(), EvaluationError> {
    for metric in [
        &value.elapsed_micros,
        &value.cost_microunits,
        &value.input_tokens,
        &value.output_tokens,
        &value.cpu_micros,
        &value.memory_high_water_bytes,
    ] {
        distribution(writer, metric)?;
    }
    Ok(())
}

fn distribution(
    writer: &mut CanonicalWriter,
    value: &MetricAvailability<DistributionSummary>,
) -> Result<(), EvaluationError> {
    if unavailable(writer, value)? {
        return Ok(());
    }
    let value = *value.value().ok_or_else(corruption)?;
    writer.write_u32(value.count()).map_err(codec)?;
    writer.write_u32(value.missing()).map_err(codec)?;
    for item in [
        value.total(),
        value.minimum(),
        value.maximum(),
        value.mean(),
        value.p50(),
        value.p95(),
        value.p99(),
    ] {
        writer.write_u64(item).map_err(codec)?;
    }
    Ok(())
}

fn reliability(
    writer: &mut CanonicalWriter,
    value: crate::EvaluationReliability,
) -> Result<(), EvaluationError> {
    counts(writer, value.counts())?;
    writer.write_u32(value.attempts()).map_err(codec)?;
    writer.write_u32(value.retried_rollouts()).map_err(codec)?;
    writer.write_u32(value.complete_trace_rollouts()).map_err(codec)?;
    writer.write_u32(value.complete_teardown_rollouts()).map_err(codec)?;
    interval(writer, &value.evaluated_interval())?;
    interval(writer, &value.infrastructure_interval())
}

const fn codec(_: peritus_codec::CodecError) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::LimitExceeded,
        EvaluationOperation::Analyze,
        EvaluationRecovery::ReduceScope,
        "canonical evaluation report exceeds production codec limits",
    )
}

const fn corruption() -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Corruption,
        EvaluationOperation::Analyze,
        EvaluationRecovery::Quarantine,
        "metric availability encoding is internally inconsistent",
    )
}
