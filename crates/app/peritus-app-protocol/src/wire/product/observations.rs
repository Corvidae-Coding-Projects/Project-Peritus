//! Each run carries its own settlement discriminator.

use super::{read_settlement_snapshot, read_snapshot, write_settlement_snapshot, write_snapshot};
use crate::MAX_PRODUCT_RUN_PAGE;
use crate::ProductRunObservation;
use crate::wire::primitive::invalid;
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};

pub(in crate::wire) fn write_observations(
    writer: &mut CanonicalWriter,
    values: &[ProductRunObservation],
) -> Result<(), CodecError> {
    if values.len() > MAX_PRODUCT_RUN_PAGE {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, writer.len()));
    }
    writer.write_collection_len(values.len())?;
    for value in values {
        writer.write_option_tag(value.settlement().is_some())?;
        if let Some(settlement) = value.settlement() {
            let settled = invalid(
                writer.len(),
                crate::ProductRunSettlementSnapshot::new(value.snapshot().clone(), settlement),
            )?;
            write_settlement_snapshot(writer, &settled)?;
        } else {
            write_snapshot(writer, value.snapshot())?;
        }
    }
    Ok(())
}

pub(in crate::wire) fn read_observations(
    reader: &mut CanonicalReader<'_>,
) -> Result<Vec<ProductRunObservation>, CodecError> {
    let offset = reader.offset();
    // Settlement tag, identifiers, providers, phase/cycle, text prefixes, operation, deliverable tag.
    let count =
        reader.read_collection_len(1 + 2 * 16 + 3 * 16 + 2 + 4 + 6 * 4 + 2 * 2 + 3 * 4 + 7 + 1)?;
    if count > MAX_PRODUCT_RUN_PAGE {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut observations = reader.reserve_collection(count)?;
    for _ in 0..count {
        let (snapshot, settlement) = if reader.read_option_tag()? {
            let value = read_settlement_snapshot(reader)?;
            (value.snapshot().clone(), Some(*value.settlement()))
        } else {
            (read_snapshot(reader)?, None)
        };
        observations.push(invalid(offset, ProductRunObservation::new(snapshot, settlement))?);
    }
    Ok(observations)
}
