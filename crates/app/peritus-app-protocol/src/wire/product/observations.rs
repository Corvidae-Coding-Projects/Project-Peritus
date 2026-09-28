//! Each run carries its own settlement discriminator; legacy bytes remain unchanged.

use super::{read_settlement_snapshot, read_snapshot, write_settlement_snapshot, write_snapshot};
use crate::MAX_PRODUCT_RUNS;
use crate::ProductRunObservation;
use crate::wire::primitive::invalid;
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};

pub(in crate::wire) fn write_observations(
    writer: &mut CanonicalWriter,
    values: &[ProductRunObservation],
) -> Result<(), CodecError> {
    if values.len() > MAX_PRODUCT_RUNS {
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
    let count = reader.read_collection_len()?;
    if count > MAX_PRODUCT_RUNS {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    (0..count)
        .map(|_| {
            let (snapshot, settlement) = if reader.read_option_tag()? {
                let value = read_settlement_snapshot(reader)?;
                (value.snapshot().clone(), Some(*value.settlement()))
            } else {
                (read_snapshot(reader)?, None)
            };
            invalid(offset, ProductRunObservation::new(snapshot, settlement))
        })
        .collect()
}
