//! Canonical deterministic attribution records.

use peritus_codec::{CanonicalReader, CanonicalWriter};

use crate::{
    AttributionEntry, AttributionRecord, AttributionUnavailable, ChangeManifestId, EvolutionError,
    EvolutionLimits, FalsificationVerdict, InteractionGroupId, MetricObservation, VariantId,
    limits::{ATTRIBUTION_PAGE_ENTRIES, represented_attribution_entries},
};

use super::{super::scalar, change, evaluation};

const PAGED_ENTRIES_VERSION: u8 = 2;
const MINIMUM_ENTRY_BYTES: usize = 16 + 32 + 2 + 1 + 2;

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
    let count = represented_attribution_entries(value.entries().len())
        .ok_or_else(scalar::protocol)?;
    if value.entries().len() <= ATTRIBUTION_PAGE_ENTRIES {
        writer.write_collection_len(value.entries().len()).map_err(scalar::codec)?;
        write_entries(writer, value.entries())?;
    } else {
        writer.write_collection_len(0).map_err(scalar::codec)?;
        writer.write_u8(PAGED_ENTRIES_VERSION).map_err(scalar::codec)?;
        writer.write_u32(count).map_err(scalar::codec)?;
        for page in value.entries().chunks(ATTRIBUTION_PAGE_ENTRIES) {
            writer.write_u16_collection_len(page.len()).map_err(scalar::codec)?;
            write_entries(writer, page)?;
        }
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
    let legacy_length =
        reader.read_collection_len(MINIMUM_ENTRY_BYTES).map_err(scalar::codec)?;
    let entries = if legacy_length == 0 {
        read_paged_entries(reader, limits)?
    } else {
        let mut entries = reader.reserve_collection(legacy_length).map_err(scalar::codec)?;
        read_entries(reader, legacy_length, &mut entries)?;
        entries
    };
    AttributionRecord::from_exact_parts(variant, evaluation_digest, interaction, entries, limits)
}

fn write_entries(
    writer: &mut CanonicalWriter,
    entries: &[AttributionEntry],
) -> Result<(), EvolutionError> {
    for entry in entries {
        writer.write_fixed(entry.manifest_id().as_bytes()).map_err(scalar::codec)?;
        writer.write_fixed(entry.prediction_digest().as_bytes()).map_err(scalar::codec)?;
        write_observation(writer, entry.observation())?;
        writer.write_u8(verdict_tag(entry.verdict())).map_err(scalar::codec)?;
        writer.write_bool(entry.mandatory()).map_err(scalar::codec)?;
        writer.write_bool(entry.critical()).map_err(scalar::codec)?;
    }
    Ok(())
}

fn read_paged_entries(
    reader: &mut CanonicalReader<'_>,
    limits: EvolutionLimits,
) -> Result<Vec<AttributionEntry>, EvolutionError> {
    if reader.read_u8().map_err(scalar::codec)? != PAGED_ENTRIES_VERSION {
        return Err(scalar::protocol());
    }
    let total = usize::try_from(reader.read_u32().map_err(scalar::codec)?)
        .map_err(|_| scalar::protocol())?;
    if total <= ATTRIBUTION_PAGE_ENTRIES || !limits.accepts_attribution_entries(total) {
        return Err(scalar::protocol());
    }
    let page_count = total.div_ceil(ATTRIBUTION_PAGE_ENTRIES);
    let minimum_bytes = total
        .checked_mul(MINIMUM_ENTRY_BYTES)
        .and_then(|bytes| page_count.checked_mul(2).and_then(|headers| bytes.checked_add(headers)))
        .ok_or_else(scalar::protocol)?;
    if minimum_bytes > reader.remaining() {
        return Err(scalar::protocol());
    }
    let mut entries = reader.reserve_collection(total).map_err(scalar::codec)?;
    while entries.len() < total {
        let remaining = total.checked_sub(entries.len()).ok_or_else(scalar::protocol)?;
        let expected = remaining.min(ATTRIBUTION_PAGE_ENTRIES);
        let length = reader
            .read_u16_collection_len(MINIMUM_ENTRY_BYTES)
            .map_err(scalar::codec)?;
        if length != expected {
            return Err(scalar::protocol());
        }
        read_entries(reader, length, &mut entries)?;
    }
    Ok(entries)
}

fn read_entries(
    reader: &mut CanonicalReader<'_>,
    length: usize,
    entries: &mut Vec<AttributionEntry>,
) -> Result<(), EvolutionError> {
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
    Ok(())
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
