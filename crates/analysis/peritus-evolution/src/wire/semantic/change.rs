//! Canonical change-manifest and isolated-variant semantics.

use peritus_codec::{CanonicalReader, CanonicalWriter};
use peritus_eval::TaskId;

use crate::{
    BoundedText, ChangeManifest, ChangeManifestId, CompatibilityEffect, ComponentDelta,
    EvolutionError, EvolutionLimits, InteractionGroupId, MetricValue, Prediction,
    PredictionDirection, PredictionMetric, PredictionSubject, VariantDefinition,
};

use super::{super::scalar, binding};

fn write_text(writer: &mut CanonicalWriter, value: &BoundedText) -> Result<(), EvolutionError> {
    writer.write_str(value.as_str()).map_err(scalar::codec)
}

fn text(
    reader: &mut CanonicalReader<'_>,
    limits: EvolutionLimits,
) -> Result<BoundedText, EvolutionError> {
    BoundedText::new(reader.read_str().map_err(scalar::codec)?.to_owned(), limits)
}

fn write_delta(writer: &mut CanonicalWriter, value: &ComponentDelta) -> Result<(), EvolutionError> {
    writer.write_str(value.component_id().as_str()).map_err(scalar::codec)?;
    writer.write_u8(value.kind().tag()).map_err(scalar::codec)?;
    writer.write_fixed(value.before_content().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.after_content().as_bytes()).map_err(scalar::codec)?;
    write_digest_option(writer, value.before_executable())?;
    write_digest_option(writer, value.after_executable())?;
    writer.write_fixed(value.semantic_diff_artifact().as_bytes()).map_err(scalar::codec)?;
    writer.write_u8(compatibility_tag(value.compatibility())).map_err(scalar::codec)?;
    write_digest_option(writer, value.migration_artifact())
}

fn delta(reader: &mut CanonicalReader<'_>) -> Result<ComponentDelta, EvolutionError> {
    ComponentDelta::from_exact_parts(
        scalar::component_id(reader)?,
        scalar::component_kind(reader)?,
        scalar::digest(reader)?,
        scalar::digest(reader)?,
        digest_option(reader)?,
        digest_option(reader)?,
        scalar::digest(reader)?,
        compatibility(reader.read_u8().map_err(scalar::codec)?)?,
        digest_option(reader)?,
    )
}

fn write_prediction(
    writer: &mut CanonicalWriter,
    value: &Prediction,
) -> Result<(), EvolutionError> {
    match value.subject() {
        PredictionSubject::Campaign => writer.write_u8(1).map_err(scalar::codec)?,
        PredictionSubject::Task(id) => {
            writer.write_u8(2).map_err(scalar::codec)?;
            writer.write_fixed(id.as_bytes()).map_err(scalar::codec)?;
        }
        PredictionSubject::FailureClass(digest) => {
            writer.write_u8(3).map_err(scalar::codec)?;
            writer.write_fixed(digest.as_bytes()).map_err(scalar::codec)?;
        }
    }
    writer.write_u8(value.metric().tag()).map_err(scalar::codec)?;
    if let PredictionMetric::TaskPassAtK(k) = value.metric() {
        writer.write_u16(k).map_err(scalar::codec)?;
    }
    writer.write_u8(direction_tag(value.direction())).map_err(scalar::codec)?;
    write_metric_value(writer, value.threshold())?;
    write_text(writer, value.rationale())?;
    writer.write_bool(value.mandatory()).map_err(scalar::codec)?;
    writer.write_bool(value.critical()).map_err(scalar::codec)
}

fn prediction(
    reader: &mut CanonicalReader<'_>,
    limits: EvolutionLimits,
) -> Result<Prediction, EvolutionError> {
    let subject = match reader.read_u8().map_err(scalar::codec)? {
        1 => PredictionSubject::Campaign,
        2 => PredictionSubject::Task(
            TaskId::new(reader.read_fixed().map_err(scalar::codec)?).map_err(scalar::domain)?,
        ),
        3 => PredictionSubject::FailureClass(scalar::digest(reader)?),
        _ => return Err(scalar::protocol()),
    };
    let metric = prediction_metric(reader)?;
    let direction = match reader.read_u8().map_err(scalar::codec)? {
        1 => PredictionDirection::AtLeast,
        2 => PredictionDirection::AtMost,
        3 => PredictionDirection::Equal,
        _ => return Err(scalar::protocol()),
    };
    let threshold = metric_value(reader)?;
    let rationale = text(reader, limits)?;
    Prediction::new(
        subject,
        metric,
        direction,
        threshold,
        rationale,
        reader.read_bool().map_err(scalar::codec)?,
        reader.read_bool().map_err(scalar::codec)?,
    )
}

pub(super) fn write_manifest(
    writer: &mut CanonicalWriter,
    value: &ChangeManifest,
) -> Result<(), EvolutionError> {
    scalar::write_harness_revision(writer, value.baseline())?;
    scalar::write_harness_revision(writer, value.candidate())?;
    write_text(writer, value.hypothesis())?;
    writer.write_collection_len(value.alternatives().len()).map_err(scalar::codec)?;
    for item in value.alternatives() {
        write_text(writer, item)?;
    }
    writer.write_collection_len(value.diagnoses().len()).map_err(scalar::codec)?;
    for item in value.diagnoses() {
        binding::write_diagnosis(writer, item)?;
    }
    writer.write_collection_len(value.deltas().len()).map_err(scalar::codec)?;
    for item in value.deltas() {
        write_delta(writer, item)?;
    }
    writer.write_collection_len(value.predictions().len()).map_err(scalar::codec)?;
    for item in value.predictions() {
        write_prediction(writer, item)?;
    }
    write_text(writer, value.falsification())?;
    scalar::write_harness_revision(writer, value.rollback_target())
}

pub(super) fn manifest(
    reader: &mut CanonicalReader<'_>,
    limits: EvolutionLimits,
) -> Result<ChangeManifest, EvolutionError> {
    let baseline = scalar::harness_revision(reader)?;
    let candidate = scalar::harness_revision(reader)?;
    let hypothesis = text(reader, limits)?;
    let alternative_len = reader.read_collection_len().map_err(scalar::codec)?;
    let mut alternatives = Vec::with_capacity(alternative_len);
    for _ in 0..alternative_len {
        alternatives.push(text(reader, limits)?);
    }
    let diagnosis_len = reader.read_collection_len().map_err(scalar::codec)?;
    let mut diagnoses = Vec::with_capacity(diagnosis_len);
    for _ in 0..diagnosis_len {
        diagnoses.push(binding::diagnosis(reader, limits)?);
    }
    let delta_len = reader.read_collection_len().map_err(scalar::codec)?;
    let mut deltas = Vec::with_capacity(delta_len);
    for _ in 0..delta_len {
        deltas.push(delta(reader)?);
    }
    let prediction_len = reader.read_collection_len().map_err(scalar::codec)?;
    let mut predictions = Vec::with_capacity(prediction_len);
    for _ in 0..prediction_len {
        predictions.push(prediction(reader, limits)?);
    }
    let falsification = text(reader, limits)?;
    let rollback = scalar::harness_revision(reader)?;
    ChangeManifest::from_exact_parts(
        baseline,
        candidate,
        hypothesis,
        alternatives,
        diagnoses,
        deltas,
        predictions,
        falsification,
        rollback,
        limits,
    )
}

pub(super) fn write_variant(
    writer: &mut CanonicalWriter,
    value: &VariantDefinition,
) -> Result<(), EvolutionError> {
    binding::write_production(writer, value.baseline())?;
    binding::write_production(writer, value.candidate())?;
    writer.write_collection_len(value.manifest_ids().len()).map_err(scalar::codec)?;
    for (id, digest) in value.manifest_ids().iter().zip(value.manifest_digests()) {
        writer.write_fixed(id.as_bytes()).map_err(scalar::codec)?;
        writer.write_fixed(digest.as_bytes()).map_err(scalar::codec)?;
    }
    writer.write_collection_len(value.changed_components().len()).map_err(scalar::codec)?;
    for component in value.changed_components() {
        writer.write_str(component.as_str()).map_err(scalar::codec)?;
    }
    writer.write_collection_len(value.changed_kinds().len()).map_err(scalar::codec)?;
    for kind in value.changed_kinds() {
        writer.write_u8(kind.tag()).map_err(scalar::codec)?;
    }
    writer.write_bool(value.changes_executable()).map_err(scalar::codec)?;
    writer.write_u8(compatibility_tag(value.compatibility())).map_err(scalar::codec)?;
    writer.write_option_tag(value.interaction_group().is_some()).map_err(scalar::codec)?;
    if let Some(group) = value.interaction_group() {
        writer.write_fixed(group.as_bytes()).map_err(scalar::codec)?;
    }
    Ok(())
}

pub(super) fn variant(
    reader: &mut CanonicalReader<'_>,
    limits: EvolutionLimits,
) -> Result<VariantDefinition, EvolutionError> {
    let baseline = binding::production(reader)?;
    let candidate = binding::production(reader)?;
    let manifest_len = reader.read_collection_len().map_err(scalar::codec)?;
    let mut ids = Vec::with_capacity(manifest_len);
    let mut digests = Vec::with_capacity(manifest_len);
    for _ in 0..manifest_len {
        ids.push(ChangeManifestId::new(reader.read_fixed().map_err(scalar::codec)?)?);
        digests.push(scalar::digest(reader)?);
    }
    let component_len = reader.read_collection_len().map_err(scalar::codec)?;
    let mut components = Vec::with_capacity(component_len);
    for _ in 0..component_len {
        components.push(scalar::component_id(reader)?);
    }
    let kind_len = reader.read_collection_len().map_err(scalar::codec)?;
    let mut kinds = Vec::with_capacity(kind_len);
    for _ in 0..kind_len {
        kinds.push(scalar::component_kind(reader)?);
    }
    let changes_executable = reader.read_bool().map_err(scalar::codec)?;
    let effect = compatibility(reader.read_u8().map_err(scalar::codec)?)?;
    let interaction = reader
        .read_option_tag()
        .map_err(scalar::codec)?
        .then(|| InteractionGroupId::new(reader.read_fixed().map_err(scalar::codec)?))
        .transpose()?;
    VariantDefinition::from_exact_parts(
        baseline,
        candidate,
        ids,
        digests,
        components,
        kinds,
        changes_executable,
        effect,
        interaction,
        limits,
    )
}

pub(super) fn write_metric_value(
    writer: &mut CanonicalWriter,
    value: MetricValue,
) -> Result<(), EvolutionError> {
    match value {
        MetricValue::SignedMillionths(item) => {
            writer.write_u8(1).map_err(scalar::codec)?;
            writer.write_fixed(&item.to_be_bytes()).map_err(scalar::codec)
        }
        MetricValue::ProbabilityMillionths(item) => {
            writer.write_u8(2).map_err(scalar::codec)?;
            writer.write_u32(item).map_err(scalar::codec)
        }
        MetricValue::Count(item) => {
            writer.write_u8(3).map_err(scalar::codec)?;
            writer.write_u32(item).map_err(scalar::codec)
        }
        MetricValue::Quantity(item) => {
            writer.write_u8(4).map_err(scalar::codec)?;
            writer.write_u64(item).map_err(scalar::codec)
        }
    }
}

pub(super) fn metric_value(
    reader: &mut CanonicalReader<'_>,
) -> Result<MetricValue, EvolutionError> {
    match reader.read_u8().map_err(scalar::codec)? {
        1 => Ok(MetricValue::SignedMillionths(i32::from_be_bytes(
            reader.read_fixed().map_err(scalar::codec)?,
        ))),
        2 => MetricValue::probability(reader.read_u32().map_err(scalar::codec)?),
        3 => Ok(MetricValue::Count(reader.read_u32().map_err(scalar::codec)?)),
        4 => Ok(MetricValue::Quantity(reader.read_u64().map_err(scalar::codec)?)),
        _ => Err(scalar::protocol()),
    }
}

fn prediction_metric(reader: &mut CanonicalReader<'_>) -> Result<PredictionMetric, EvolutionError> {
    match reader.read_u8().map_err(scalar::codec)? {
        1 => Ok(PredictionMetric::CandidateCorrectnessLower),
        2 => Ok(PredictionMetric::PairedEffectLower),
        3 => Ok(PredictionMetric::TaskPassAtK(reader.read_u16().map_err(scalar::codec)?)),
        4 => Ok(PredictionMetric::SafetyFailures),
        5 => Ok(PredictionMetric::ReliabilityLower),
        6 => Ok(PredictionMetric::LatencyP95Micros),
        7 => Ok(PredictionMetric::CostMeanMicrounits),
        8 => Ok(PredictionMetric::InputTokensMean),
        9 => Ok(PredictionMetric::OutputTokensMean),
        10 => Ok(PredictionMetric::TraceCompleteness),
        11 => Ok(PredictionMetric::TeardownCompleteness),
        _ => Err(scalar::protocol()),
    }
}

const fn direction_tag(value: PredictionDirection) -> u8 {
    match value {
        PredictionDirection::AtLeast => 1,
        PredictionDirection::AtMost => 2,
        PredictionDirection::Equal => 3,
    }
}
pub(super) const fn compatibility_tag(value: CompatibilityEffect) -> u8 {
    match value {
        CompatibilityEffect::Compatible => 1,
        CompatibilityEffect::RequiresMigration => 2,
        CompatibilityEffect::Incompatible => 3,
    }
}
pub(super) const fn compatibility(tag: u8) -> Result<CompatibilityEffect, EvolutionError> {
    match tag {
        1 => Ok(CompatibilityEffect::Compatible),
        2 => Ok(CompatibilityEffect::RequiresMigration),
        3 => Ok(CompatibilityEffect::Incompatible),
        _ => Err(scalar::protocol()),
    }
}
fn write_digest_option(
    writer: &mut CanonicalWriter,
    value: Option<peritus_types::Sha256Digest>,
) -> Result<(), EvolutionError> {
    writer.write_option_tag(value.is_some()).map_err(scalar::codec)?;
    if let Some(value) = value {
        writer.write_fixed(value.as_bytes()).map_err(scalar::codec)?;
    }
    Ok(())
}
fn digest_option(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<peritus_types::Sha256Digest>, EvolutionError> {
    reader.read_option_tag().map_err(scalar::codec)?.then(|| scalar::digest(reader)).transpose()
}
