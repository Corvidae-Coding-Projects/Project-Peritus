//! Canonical product-run request and observation encoding.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};

use crate::{
    MAX_PRODUCT_RUNS, ProductProviderSelection, ProductRunControl, ProductRunControlAction,
    ProductRunPhase, ProductRunQuery, ProductRunRequest, ProductRunSnapshot,
};

use super::primitive::{invalid, read_id, write_id};

pub(super) fn write_run_request(
    writer: &mut CanonicalWriter,
    value: &ProductRunRequest,
) -> Result<(), CodecError> {
    write_id(writer, value.run_id().as_bytes())?;
    write_id(writer, value.workspace_id().as_bytes())?;
    write_providers(writer, value.providers())?;
    writer.write_str(value.task())
}

pub(super) fn read_run_request(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductRunRequest, CodecError> {
    let offset = reader.offset();
    invalid(
        offset,
        ProductRunRequest::new(
            read_id(reader, RunId::new)?,
            read_id(reader, WorkspaceId::new)?,
            read_providers(reader)?,
            reader.read_str()?.to_owned(),
        ),
    )
}

pub(super) fn write_run_control(
    writer: &mut CanonicalWriter,
    value: ProductRunControl,
) -> Result<(), CodecError> {
    write_id(writer, value.run_id().as_bytes())?;
    writer.write_u16(value.action().tag())
}

pub(super) fn read_run_control(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductRunControl, CodecError> {
    let run_id = read_id(reader, RunId::new)?;
    let offset = reader.offset();
    let action = ProductRunControlAction::from_tag(reader.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, offset))?;
    Ok(ProductRunControl::new(run_id, action))
}

pub(super) fn write_run_query(
    writer: &mut CanonicalWriter,
    value: ProductRunQuery,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.run_id().is_some())?;
    if let Some(run_id) = value.run_id() {
        write_id(writer, run_id.as_bytes())?;
    }
    Ok(())
}

pub(super) fn read_run_query(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductRunQuery, CodecError> {
    if reader.read_option_tag()? {
        Ok(ProductRunQuery::exact(read_id(reader, RunId::new)?))
    } else {
        Ok(ProductRunQuery::recent())
    }
}

pub(super) fn write_snapshot(
    writer: &mut CanonicalWriter,
    value: &ProductRunSnapshot,
) -> Result<(), CodecError> {
    write_id(writer, value.run_id().as_bytes())?;
    write_id(writer, value.workspace_id().as_bytes())?;
    write_providers(writer, value.providers())?;
    writer.write_u16(value.phase().tag())?;
    writer.write_u32(value.cycle())?;
    for text in
        [value.task(), value.status(), value.diff(), value.gates(), value.review(), value.summary()]
    {
        writer.write_str(text)?;
    }
    Ok(())
}

pub(super) fn read_snapshot(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductRunSnapshot, CodecError> {
    let offset = reader.offset();
    let run_id = read_id(reader, RunId::new)?;
    let workspace_id = read_id(reader, WorkspaceId::new)?;
    let providers = read_providers(reader)?;
    let phase_offset = reader.offset();
    let phase = ProductRunPhase::from_tag(reader.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, phase_offset))?;
    let cycle = reader.read_u32()?;
    let task = reader.read_str()?.to_owned();
    let status = reader.read_str()?.to_owned();
    let diff = reader.read_str()?.to_owned();
    let gates = reader.read_str()?.to_owned();
    let review = reader.read_str()?.to_owned();
    let summary = reader.read_str()?.to_owned();
    invalid(
        offset,
        ProductRunSnapshot::new(
            run_id,
            workspace_id,
            providers,
            phase,
            cycle,
            task,
            status,
            diff,
            gates,
            review,
            summary,
        ),
    )
}

pub(super) fn write_snapshots(
    writer: &mut CanonicalWriter,
    values: &[ProductRunSnapshot],
) -> Result<(), CodecError> {
    writer.write_collection_len(values.len())?;
    for value in values {
        write_snapshot(writer, value)?;
    }
    Ok(())
}

pub(super) fn read_snapshots(
    reader: &mut CanonicalReader<'_>,
) -> Result<Vec<ProductRunSnapshot>, CodecError> {
    let offset = reader.offset();
    let length = reader.read_collection_len()?;
    if length > MAX_PRODUCT_RUNS {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    (0..length).map(|_| read_snapshot(reader)).collect()
}

fn write_providers(
    writer: &mut CanonicalWriter,
    value: ProductProviderSelection,
) -> Result<(), CodecError> {
    for profile in [value.writer(), value.reviewer(), value.fixer()] {
        write_id(writer, profile.as_bytes())?;
    }
    Ok(())
}

fn read_providers(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductProviderSelection, CodecError> {
    Ok(ProductProviderSelection::new(
        read_id(reader, ProviderProfileId::new)?,
        read_id(reader, ProviderProfileId::new)?,
        read_id(reader, ProviderProfileId::new)?,
    ))
}
