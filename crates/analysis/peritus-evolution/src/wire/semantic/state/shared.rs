//! Shared bounded collection and option primitives for complete checkpoints.

use peritus_codec::{CanonicalReader, CanonicalWriter};

use crate::EvolutionError;

use super::super::super::scalar;

pub(super) fn read_vec<T>(
    reader: &mut CanonicalReader<'_>,
    maximum: usize,
    mut read: impl FnMut(&mut CanonicalReader<'_>) -> Result<T, EvolutionError>,
) -> Result<Vec<T>, EvolutionError> {
    let length = reader.read_collection_len().map_err(scalar::codec)?;
    if length > maximum {
        return Err(scalar::protocol());
    }
    let mut values = Vec::with_capacity(length);
    for _ in 0..length {
        values.push(read(reader)?);
    }
    Ok(values)
}

pub(super) fn write_option<T>(
    writer: &mut CanonicalWriter,
    value: Option<&T>,
    write: impl FnOnce(&mut CanonicalWriter, &T) -> Result<(), EvolutionError>,
) -> Result<(), EvolutionError> {
    writer.write_option_tag(value.is_some()).map_err(scalar::codec)?;
    if let Some(value) = value {
        write(writer, value)?;
    }
    Ok(())
}

pub(super) fn read_option<T>(
    reader: &mut CanonicalReader<'_>,
    read: impl FnOnce(&mut CanonicalReader<'_>) -> Result<T, EvolutionError>,
) -> Result<Option<T>, EvolutionError> {
    reader.read_option_tag().map_err(scalar::codec)?.then(|| read(reader)).transpose()
}
