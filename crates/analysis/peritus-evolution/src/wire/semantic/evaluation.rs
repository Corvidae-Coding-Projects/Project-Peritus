//! Restart-complete E3 evidence bridge codec.

use peritus_codec::{CanonicalReader, CanonicalWriter};
use peritus_eval::{
    DatasetDigest, EvaluationCampaignId, EvaluationPlanId, EvaluationReportId, HarnessArmBinding,
    MetricUnavailableReason, PlanDigest, ProfileDigest, ResultDigest, TaskId,
};
use peritus_types::EvidenceId;

use crate::{
    EvaluationAnalysisSnapshot, EvaluationMetric, EvolutionError, PublishedEvaluationEvidence,
    TaskPassAtKSnapshot,
};

use super::super::scalar;

pub(super) fn write(
    writer: &mut CanonicalWriter,
    value: &PublishedEvaluationEvidence,
) -> Result<(), EvolutionError> {
    writer.write_fixed(value.campaign_id().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.dataset_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.profile_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.plan_id().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.plan_digest().as_bytes()).map_err(scalar::codec)?;
    write_arm(writer, value.baseline())?;
    write_arm(writer, value.candidate())?;
    writer.write_fixed(value.report_id().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.report_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.report_artifact().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.evidence_id().as_bytes()).map_err(scalar::codec)?;
    writer.write_u64(value.journal_position()).map_err(scalar::codec)?;
    write_analysis(writer, value.analysis())
}

pub(super) fn read(
    reader: &mut CanonicalReader<'_>,
) -> Result<PublishedEvaluationEvidence, EvolutionError> {
    let campaign = EvaluationCampaignId::new(reader.read_fixed().map_err(scalar::codec)?)
        .map_err(scalar::domain)?;
    let dataset = DatasetDigest::new(scalar::digest(reader)?);
    let profile = ProfileDigest::new(scalar::digest(reader)?);
    let plan_id = EvaluationPlanId::new(reader.read_fixed().map_err(scalar::codec)?)
        .map_err(scalar::domain)?;
    let plan_digest = PlanDigest::new(scalar::digest(reader)?);
    let baseline = arm(reader)?;
    let candidate = arm(reader)?;
    let report = EvaluationReportId::new(reader.read_fixed().map_err(scalar::codec)?)
        .map_err(scalar::domain)?;
    let report_digest = scalar::digest(reader)?;
    let artifact = scalar::digest(reader)?;
    let evidence =
        EvidenceId::new(reader.read_fixed().map_err(scalar::codec)?).map_err(scalar::domain)?;
    let position = reader.read_u64().map_err(scalar::codec)?;
    let analysis = analysis(reader)?;
    PublishedEvaluationEvidence::from_exact_parts(
        campaign,
        dataset,
        profile,
        plan_id,
        plan_digest,
        baseline,
        candidate,
        report,
        report_digest,
        artifact,
        evidence,
        position,
        analysis,
    )
}

fn write_arm(writer: &mut CanonicalWriter, value: HarnessArmBinding) -> Result<(), EvolutionError> {
    scalar::write_revision(writer, value.revision())?;
    scalar::write_harness_revision(writer, value.harness_revision())?;
    writer.write_fixed(value.receipt_digest().as_bytes()).map_err(scalar::codec)
}

fn arm(reader: &mut CanonicalReader<'_>) -> Result<HarnessArmBinding, EvolutionError> {
    let revision = scalar::revision(reader)?;
    let harness = scalar::harness_revision(reader)?;
    if revision.harness_id() != harness.harness_id() {
        return Err(scalar::protocol());
    }
    Ok(HarnessArmBinding::new(revision, harness, scalar::digest(reader)?))
}

fn write_analysis(
    writer: &mut CanonicalWriter,
    value: &EvaluationAnalysisSnapshot,
) -> Result<(), EvolutionError> {
    writer.write_fixed(value.source_digest().as_bytes()).map_err(scalar::codec)?;
    write_u32_metric(writer, value.candidate_correctness_lower())?;
    match value.candidate_pass_at_k() {
        EvaluationMetric::Available(values) => {
            writer.write_u8(1).map_err(scalar::codec)?;
            writer.write_collection_len(values.len()).map_err(scalar::codec)?;
            for item in values {
                writer.write_fixed(item.task_id().as_bytes()).map_err(scalar::codec)?;
                writer.write_u16(item.k()).map_err(scalar::codec)?;
                writer.write_u32(item.estimate_millionths()).map_err(scalar::codec)?;
            }
        }
        EvaluationMetric::Unavailable(reason) => write_unavailable(writer, *reason)?,
    }
    match value.paired_effect_lower() {
        EvaluationMetric::Available(item) => {
            writer.write_u8(1).map_err(scalar::codec)?;
            writer.write_fixed(&item.to_be_bytes()).map_err(scalar::codec)?;
        }
        EvaluationMetric::Unavailable(reason) => write_unavailable(writer, reason)?,
    }
    writer.write_u32(value.candidate_safety_failures()).map_err(scalar::codec)?;
    write_u32_metric(writer, value.reliability_lower())?;
    for metric in [
        value.latency_p95_micros(),
        value.cost_mean_microunits(),
        value.input_tokens_mean(),
        value.output_tokens_mean(),
    ] {
        write_u64_metric(writer, metric)?;
    }
    writer.write_u32(value.expected_rollouts()).map_err(scalar::codec)?;
    writer.write_u32(value.complete_trace_rollouts()).map_err(scalar::codec)?;
    writer.write_u32(value.complete_teardown_rollouts()).map_err(scalar::codec)
}

fn analysis(
    reader: &mut CanonicalReader<'_>,
) -> Result<EvaluationAnalysisSnapshot, EvolutionError> {
    let source = ResultDigest::new(scalar::digest(reader)?);
    let correctness = read_u32_metric(reader)?;
    let pass_at_k = match reader.read_u8().map_err(scalar::codec)? {
        1 => {
            let length = reader.read_collection_len().map_err(scalar::codec)?;
            let mut values = Vec::with_capacity(length);
            for _ in 0..length {
                values.push(TaskPassAtKSnapshot::new(
                    TaskId::new(reader.read_fixed().map_err(scalar::codec)?)
                        .map_err(scalar::domain)?,
                    reader.read_u16().map_err(scalar::codec)?,
                    reader.read_u32().map_err(scalar::codec)?,
                ));
            }
            if values.is_empty()
                || values.windows(2).any(|pair| pair[0] >= pair[1])
                || values
                    .iter()
                    .any(|value| value.k() == 0 || value.estimate_millionths() > 1_000_000)
            {
                return Err(scalar::protocol());
            }
            EvaluationMetric::Available(values)
        }
        2 => EvaluationMetric::Unavailable(reason(reader)?),
        _ => return Err(scalar::protocol()),
    };
    let paired = match reader.read_u8().map_err(scalar::codec)? {
        1 => EvaluationMetric::Available(i32::from_be_bytes(
            reader.read_fixed().map_err(scalar::codec)?,
        )),
        2 => EvaluationMetric::Unavailable(reason(reader)?),
        _ => return Err(scalar::protocol()),
    };
    let safety = reader.read_u32().map_err(scalar::codec)?;
    let reliability = read_u32_metric(reader)?;
    let latency = read_u64_metric(reader)?;
    let cost = read_u64_metric(reader)?;
    let input = read_u64_metric(reader)?;
    let output = read_u64_metric(reader)?;
    let expected = reader.read_u32().map_err(scalar::codec)?;
    let traces = reader.read_u32().map_err(scalar::codec)?;
    let teardown = reader.read_u32().map_err(scalar::codec)?;
    if correctness.value().is_some_and(|value| value > 1_000_000)
        || reliability.value().is_some_and(|value| value > 1_000_000)
        || paired.value().is_some_and(|value| !(-1_000_000..=1_000_000).contains(&value))
        || expected == 0
        || traces > expected
        || teardown > expected
    {
        return Err(scalar::protocol());
    }
    Ok(EvaluationAnalysisSnapshot::from_exact_parts(
        source,
        correctness,
        pass_at_k,
        paired,
        safety,
        reliability,
        latency,
        cost,
        input,
        output,
        expected,
        traces,
        teardown,
    ))
}

fn write_u32_metric(
    writer: &mut CanonicalWriter,
    value: EvaluationMetric<u32>,
) -> Result<(), EvolutionError> {
    match value {
        EvaluationMetric::Available(item) => {
            writer.write_u8(1).map_err(scalar::codec)?;
            writer.write_u32(item).map_err(scalar::codec)
        }
        EvaluationMetric::Unavailable(reason) => write_unavailable(writer, reason),
    }
}

fn read_u32_metric(
    reader: &mut CanonicalReader<'_>,
) -> Result<EvaluationMetric<u32>, EvolutionError> {
    match reader.read_u8().map_err(scalar::codec)? {
        1 => Ok(EvaluationMetric::Available(reader.read_u32().map_err(scalar::codec)?)),
        2 => Ok(EvaluationMetric::Unavailable(reason(reader)?)),
        _ => Err(scalar::protocol()),
    }
}

fn write_u64_metric(
    writer: &mut CanonicalWriter,
    value: EvaluationMetric<u64>,
) -> Result<(), EvolutionError> {
    match value {
        EvaluationMetric::Available(item) => {
            writer.write_u8(1).map_err(scalar::codec)?;
            writer.write_u64(item).map_err(scalar::codec)
        }
        EvaluationMetric::Unavailable(reason) => write_unavailable(writer, reason),
    }
}

fn read_u64_metric(
    reader: &mut CanonicalReader<'_>,
) -> Result<EvaluationMetric<u64>, EvolutionError> {
    match reader.read_u8().map_err(scalar::codec)? {
        1 => Ok(EvaluationMetric::Available(reader.read_u64().map_err(scalar::codec)?)),
        2 => Ok(EvaluationMetric::Unavailable(reason(reader)?)),
        _ => Err(scalar::protocol()),
    }
}

fn write_unavailable(
    writer: &mut CanonicalWriter,
    reason: MetricUnavailableReason,
) -> Result<(), EvolutionError> {
    writer.write_u8(2).map_err(scalar::codec)?;
    writer.write_u8(crate::binding::reason_tag(reason)).map_err(scalar::codec)
}

pub(super) fn reason(
    reader: &mut CanonicalReader<'_>,
) -> Result<MetricUnavailableReason, EvolutionError> {
    match reader.read_u8().map_err(scalar::codec)? {
        1 => Ok(MetricUnavailableReason::IncompleteLedger),
        2 => Ok(MetricUnavailableReason::CancelledRollout),
        3 => Ok(MetricUnavailableReason::AmbiguousRollout),
        4 => Ok(MetricUnavailableReason::InfrastructureInvalidated),
        5 => Ok(MetricUnavailableReason::EmptyDenominator),
        6 => Ok(MetricUnavailableReason::RequiredObservationMissing),
        _ => Err(scalar::protocol()),
    }
}
