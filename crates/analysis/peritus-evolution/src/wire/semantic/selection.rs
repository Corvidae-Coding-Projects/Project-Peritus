//! Canonical deny-wins assessments and selection decisions.

use peritus_codec::{CanonicalReader, CanonicalWriter};

use crate::{
    AttributionId, Criterion, CriterionOutcome, CriterionResult, EvolutionError, EvolutionLimits,
    ObjectiveVector, SelectionDecision, SelectionRecord, VariantAssessment, VariantId,
    VariantRejection,
};

use super::{super::scalar, change};

pub(super) fn write_assessment(
    writer: &mut CanonicalWriter,
    value: &VariantAssessment,
) -> Result<(), EvolutionError> {
    writer.write_fixed(value.variant_id().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.attribution_id().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.evidence_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.policy_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_collection_len(value.criteria().len()).map_err(scalar::codec)?;
    for result in value.criteria() {
        writer.write_u8(result.criterion().tag()).map_err(scalar::codec)?;
        writer.write_u8(outcome_tag(result.outcome())).map_err(scalar::codec)?;
        writer.write_option_tag(result.observed().is_some()).map_err(scalar::codec)?;
        if let Some(observed) = result.observed() {
            change::write_metric_value(writer, observed)?;
        }
        writer.write_fixed(result.evidence_digest().as_bytes()).map_err(scalar::codec)?;
    }
    write_objectives(writer, value.objectives())
}

pub(super) fn assessment(
    reader: &mut CanonicalReader<'_>,
    limits: EvolutionLimits,
) -> Result<VariantAssessment, EvolutionError> {
    let variant = VariantId::new(reader.read_fixed().map_err(scalar::codec)?)?;
    let attribution = AttributionId::new(reader.read_fixed().map_err(scalar::codec)?)?;
    let evidence = scalar::digest(reader)?;
    let policy = scalar::digest(reader)?;
    let length = reader.read_collection_len().map_err(scalar::codec)?;
    if length != 14 || length > usize::from(limits.criteria()) {
        return Err(scalar::protocol());
    }
    let mut criteria = Vec::with_capacity(length);
    for expected in 0_u8..14 {
        let criterion = criterion(reader.read_u8().map_err(scalar::codec)?)?;
        if criterion.tag() != expected {
            return Err(scalar::protocol());
        }
        let outcome = outcome(reader.read_u8().map_err(scalar::codec)?)?;
        let observed = reader
            .read_option_tag()
            .map_err(scalar::codec)?
            .then(|| change::metric_value(reader))
            .transpose()?;
        criteria.push(CriterionResult::new(criterion, outcome, observed, scalar::digest(reader)?));
    }
    Ok(VariantAssessment::from_exact_parts(
        variant,
        attribution,
        evidence,
        policy,
        criteria,
        objectives(reader)?,
    ))
}

pub(super) fn write_selection(
    writer: &mut CanonicalWriter,
    value: &SelectionRecord,
) -> Result<(), EvolutionError> {
    writer.write_fixed(value.policy_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_collection_len(value.assessment_digests().len()).map_err(scalar::codec)?;
    for digest in value.assessment_digests() {
        writer.write_fixed(digest.as_bytes()).map_err(scalar::codec)?;
    }
    match value.decision() {
        SelectionDecision::Selected(id) => {
            writer.write_u8(1).map_err(scalar::codec)?;
            writer.write_fixed(id.as_bytes()).map_err(scalar::codec)?;
        }
        SelectionDecision::NoEligibleVariant(rejections) => {
            writer.write_u8(2).map_err(scalar::codec)?;
            writer.write_collection_len(rejections.len()).map_err(scalar::codec)?;
            for rejection in rejections {
                writer.write_fixed(rejection.variant_id().as_bytes()).map_err(scalar::codec)?;
                write_criteria(writer, rejection.failed())?;
                write_criteria(writer, rejection.unavailable())?;
            }
        }
    }
    Ok(())
}

pub(super) fn selection(
    reader: &mut CanonicalReader<'_>,
) -> Result<SelectionRecord, EvolutionError> {
    let policy = scalar::digest(reader)?;
    let digest_len = reader.read_collection_len().map_err(scalar::codec)?;
    if digest_len == 0 {
        return Err(scalar::protocol());
    }
    let mut digests = Vec::with_capacity(digest_len);
    for _ in 0..digest_len {
        digests.push(scalar::digest(reader)?);
    }
    let decision = match reader.read_u8().map_err(scalar::codec)? {
        1 => SelectionDecision::Selected(VariantId::new(
            reader.read_fixed().map_err(scalar::codec)?,
        )?),
        2 => {
            let length = reader.read_collection_len().map_err(scalar::codec)?;
            if length == 0 {
                return Err(scalar::protocol());
            }
            let mut rejections = Vec::with_capacity(length);
            for _ in 0..length {
                let variant = VariantId::new(reader.read_fixed().map_err(scalar::codec)?)?;
                let failed = read_criteria(reader)?;
                let unavailable = read_criteria(reader)?;
                if failed.is_empty() && unavailable.is_empty()
                    || failed.iter().any(|item| unavailable.binary_search(item).is_ok())
                {
                    return Err(scalar::protocol());
                }
                rejections.push(VariantRejection::new(variant, failed, unavailable));
            }
            if rejections.windows(2).any(|pair| pair[0].variant_id() >= pair[1].variant_id()) {
                return Err(scalar::protocol());
            }
            SelectionDecision::NoEligibleVariant(rejections)
        }
        _ => return Err(scalar::protocol()),
    };
    Ok(SelectionRecord::from_exact_parts(policy, digests, decision))
}

fn write_objectives(
    writer: &mut CanonicalWriter,
    value: ObjectiveVector,
) -> Result<(), EvolutionError> {
    writer.write_fixed(&value.paired_lower().to_be_bytes()).map_err(scalar::codec)?;
    writer.write_u32(value.critical_regressions()).map_err(scalar::codec)?;
    writer.write_u32(value.safety_failures()).map_err(scalar::codec)?;
    writer.write_u32(value.reliability_lower()).map_err(scalar::codec)?;
    writer.write_u64(value.latency_p95()).map_err(scalar::codec)?;
    writer.write_u64(value.cost_mean()).map_err(scalar::codec)?;
    writer.write_u64(value.input_tokens_mean()).map_err(scalar::codec)?;
    writer.write_u64(value.output_tokens_mean()).map_err(scalar::codec)?;
    writer.write_u32(value.attribution_coverage()).map_err(scalar::codec)
}

fn objectives(reader: &mut CanonicalReader<'_>) -> Result<ObjectiveVector, EvolutionError> {
    Ok(ObjectiveVector {
        paired_lower: i32::from_be_bytes(reader.read_fixed().map_err(scalar::codec)?),
        critical_regressions: reader.read_u32().map_err(scalar::codec)?,
        safety_failures: reader.read_u32().map_err(scalar::codec)?,
        reliability_lower: reader.read_u32().map_err(scalar::codec)?,
        latency_p95: reader.read_u64().map_err(scalar::codec)?,
        cost_mean: reader.read_u64().map_err(scalar::codec)?,
        input_tokens_mean: reader.read_u64().map_err(scalar::codec)?,
        output_tokens_mean: reader.read_u64().map_err(scalar::codec)?,
        attribution_coverage: reader.read_u32().map_err(scalar::codec)?,
    })
}

fn write_criteria(
    writer: &mut CanonicalWriter,
    values: &[Criterion],
) -> Result<(), EvolutionError> {
    writer.write_collection_len(values.len()).map_err(scalar::codec)?;
    for value in values {
        writer.write_u8(value.tag()).map_err(scalar::codec)?;
    }
    Ok(())
}

fn read_criteria(reader: &mut CanonicalReader<'_>) -> Result<Vec<Criterion>, EvolutionError> {
    let length = reader.read_collection_len().map_err(scalar::codec)?;
    let mut values = Vec::with_capacity(length);
    for _ in 0..length {
        values.push(criterion(reader.read_u8().map_err(scalar::codec)?)?);
    }
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(scalar::protocol());
    }
    Ok(values)
}

const fn criterion(tag: u8) -> Result<Criterion, EvolutionError> {
    match tag {
        0 => Ok(Criterion::PairedCorrectness),
        1 => Ok(Criterion::CriticalRegressions),
        2 => Ok(Criterion::Safety),
        3 => Ok(Criterion::Reliability),
        4 => Ok(Criterion::AttributionCoverage),
        5 => Ok(Criterion::MandatoryPredictions),
        6 => Ok(Criterion::Latency),
        7 => Ok(Criterion::Cost),
        8 => Ok(Criterion::InputTokens),
        9 => Ok(Criterion::OutputTokens),
        10 => Ok(Criterion::TraceCompleteness),
        11 => Ok(Criterion::TeardownCompleteness),
        12 => Ok(Criterion::IndependentReview),
        13 => Ok(Criterion::Compatibility),
        _ => Err(scalar::protocol()),
    }
}
const fn outcome_tag(value: CriterionOutcome) -> u8 {
    match value {
        CriterionOutcome::Passed => 1,
        CriterionOutcome::Failed => 2,
        CriterionOutcome::Unavailable => 3,
    }
}
const fn outcome(tag: u8) -> Result<CriterionOutcome, EvolutionError> {
    match tag {
        1 => Ok(CriterionOutcome::Passed),
        2 => Ok(CriterionOutcome::Failed),
        3 => Ok(CriterionOutcome::Unavailable),
        _ => Err(scalar::protocol()),
    }
}
