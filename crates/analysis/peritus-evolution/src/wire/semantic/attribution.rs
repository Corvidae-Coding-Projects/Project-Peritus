//! Canonical deterministic attribution records.

use peritus_codec::{CanonicalReader, CanonicalWriter};

use crate::{
    AttributionEntry, AttributionRecord, AttributionUnavailable, ChangeManifestId, EvolutionError,
    EvolutionLimits, FalsificationVerdict, InteractionGroupId, MetricObservation, VariantId,
};

use super::{super::scalar, change, evaluation};

pub(super) fn write(
    writer: &mut CanonicalWriter,
    value: &AttributionRecord,
) -> Result<(), EvolutionError> {
    writer.write_fixed(value.variant_id().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.evaluation_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_option_tag(value.interaction_group().is_some()).map_err(scalar::codec)?;
    if let Some(group) = value.interaction_group() {
        writer.write_fixed(group.as_bytes()).map_err(scalar::codec)?;
    }
    writer.write_collection_len(value.entries().len()).map_err(scalar::codec)?;
    for entry in value.entries() {
        writer.write_fixed(entry.manifest_id().as_bytes()).map_err(scalar::codec)?;
        writer.write_fixed(entry.prediction_digest().as_bytes()).map_err(scalar::codec)?;
        write_observation(writer, entry.observation())?;
        writer.write_u8(verdict_tag(entry.verdict())).map_err(scalar::codec)?;
        writer.write_bool(entry.mandatory()).map_err(scalar::codec)?;
        writer.write_bool(entry.critical()).map_err(scalar::codec)?;
    }
    Ok(())
}

pub(super) fn read(
    reader: &mut CanonicalReader<'_>,
    limits: EvolutionLimits,
) -> Result<AttributionRecord, EvolutionError> {
    let variant = VariantId::new(reader.read_fixed().map_err(scalar::codec)?)?;
    let evaluation_digest = scalar::digest(reader)?;
    let interaction = reader
        .read_option_tag()
        .map_err(scalar::codec)?
        .then(|| InteractionGroupId::new(reader.read_fixed().map_err(scalar::codec)?))
        .transpose()?;
    let length = reader.read_collection_len().map_err(scalar::codec)?;
    let mut entries = Vec::with_capacity(length);
    for _ in 0..length {
        entries.push(AttributionEntry::new(
            ChangeManifestId::new(reader.read_fixed().map_err(scalar::codec)?)?,
            scalar::digest(reader)?,
            observation(reader)?,
            verdict(reader.read_u8().map_err(scalar::codec)?)?,
            reader.read_bool().map_err(scalar::codec)?,
            reader.read_bool().map_err(scalar::codec)?,
        ));
    }
    AttributionRecord::from_exact_parts(variant, evaluation_digest, interaction, entries, limits)
}

fn write_observation(
    writer: &mut CanonicalWriter,
    value: MetricObservation,
) -> Result<(), EvolutionError> {
    match value {
        MetricObservation::Available(item) => {
            writer.write_u8(1).map_err(scalar::codec)?;
            change::write_metric_value(writer, item)
        }
        MetricObservation::Unavailable(reason) => {
            writer.write_u8(2).map_err(scalar::codec)?;
            match reason {
                AttributionUnavailable::Evaluation(item) => {
                    writer.write_u8(1).map_err(scalar::codec)?;
                    writer.write_u8(crate::binding::reason_tag(item)).map_err(scalar::codec)
                }
                AttributionUnavailable::TaskAbsent => writer.write_u8(2).map_err(scalar::codec),
                AttributionUnavailable::MetricAbsent => writer.write_u8(3).map_err(scalar::codec),
                AttributionUnavailable::UnsupportedFailureClass => {
                    writer.write_u8(4).map_err(scalar::codec)
                }
                AttributionUnavailable::Arithmetic => writer.write_u8(5).map_err(scalar::codec),
            }
        }
    }
}

fn observation(reader: &mut CanonicalReader<'_>) -> Result<MetricObservation, EvolutionError> {
    match reader.read_u8().map_err(scalar::codec)? {
        1 => Ok(MetricObservation::Available(change::metric_value(reader)?)),
        2 => Ok(MetricObservation::Unavailable(match reader.read_u8().map_err(scalar::codec)? {
            1 => AttributionUnavailable::Evaluation(evaluation::reason(reader)?),
            2 => AttributionUnavailable::TaskAbsent,
            3 => AttributionUnavailable::MetricAbsent,
            4 => AttributionUnavailable::UnsupportedFailureClass,
            5 => AttributionUnavailable::Arithmetic,
            _ => return Err(scalar::protocol()),
        })),
        _ => Err(scalar::protocol()),
    }
}

const fn verdict_tag(value: FalsificationVerdict) -> u8 {
    match value {
        FalsificationVerdict::Confirmed => 1,
        FalsificationVerdict::Contradicted => 2,
        FalsificationVerdict::Inconclusive => 3,
        FalsificationVerdict::NotObserved => 4,
    }
}

const fn verdict(tag: u8) -> Result<FalsificationVerdict, EvolutionError> {
    match tag {
        1 => Ok(FalsificationVerdict::Confirmed),
        2 => Ok(FalsificationVerdict::Contradicted),
        3 => Ok(FalsificationVerdict::Inconclusive),
        4 => Ok(FalsificationVerdict::NotObserved),
        _ => Err(scalar::protocol()),
    }
}
