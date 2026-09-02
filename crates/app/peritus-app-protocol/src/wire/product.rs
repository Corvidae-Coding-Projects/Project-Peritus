//! Canonical product-run request and observation encoding.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};

use crate::{
    MAX_PRODUCT_DELIVERABLE_COMMANDS, MAX_PRODUCT_DELIVERABLE_PATHS, MAX_PRODUCT_MESSAGES,
    MAX_PRODUCT_RUNS, ProductConversationMessage, ProductConversationRole, ProductDeliverable,
    ProductProviderSelection, ProductRunContinuation, ProductRunControl, ProductRunControlAction,
    ProductRunConversation, ProductRunConversationQuery, ProductRunPhase, ProductRunQuery,
    ProductRunRequest, ProductRunSnapshot,
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

pub(super) fn write_run_continuation(
    writer: &mut CanonicalWriter,
    value: &ProductRunContinuation,
) -> Result<(), CodecError> {
    write_id(writer, value.run_id().as_bytes())?;
    writer.write_str(value.message())
}

pub(super) fn read_run_continuation(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductRunContinuation, CodecError> {
    let offset = reader.offset();
    invalid(
        offset,
        ProductRunContinuation::new(read_id(reader, RunId::new)?, reader.read_str()?.to_owned()),
    )
}

pub(super) fn write_conversation_query(
    writer: &mut CanonicalWriter,
    value: ProductRunConversationQuery,
) -> Result<(), CodecError> {
    write_id(writer, value.run_id().as_bytes())
}

pub(super) fn read_conversation_query(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductRunConversationQuery, CodecError> {
    Ok(ProductRunConversationQuery::new(read_id(reader, RunId::new)?))
}

pub(super) fn write_conversation(
    writer: &mut CanonicalWriter,
    value: &ProductRunConversation,
) -> Result<(), CodecError> {
    write_id(writer, value.run_id().as_bytes())?;
    writer.write_collection_len(value.messages().len())?;
    for message in value.messages() {
        writer.write_u16(message.role().tag())?;
        writer.write_str(message.content())?;
    }
    Ok(())
}

pub(super) fn read_conversation(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductRunConversation, CodecError> {
    let offset = reader.offset();
    let run_id = read_id(reader, RunId::new)?;
    let length = reader.read_collection_len()?;
    if length > MAX_PRODUCT_MESSAGES {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut messages = Vec::with_capacity(length);
    for _ in 0..length {
        let role_offset = reader.offset();
        let role = ProductConversationRole::from_tag(reader.read_u16()?)
            .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, role_offset))?;
        messages.push(invalid(
            offset,
            ProductConversationMessage::new(role, reader.read_str()?.to_owned()),
        )?);
    }
    invalid(offset, ProductRunConversation::new(run_id, messages))
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
    writer.write_option_tag(value.deliverable().is_some())?;
    if let Some(deliverable) = value.deliverable() {
        write_deliverable(writer, deliverable)?;
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
    let snapshot = invalid(
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
    )?;
    if reader.read_option_tag()? {
        Ok(snapshot.with_deliverable(read_deliverable(reader)?))
    } else {
        Ok(snapshot)
    }
}

fn write_deliverable(
    writer: &mut CanonicalWriter,
    value: &ProductDeliverable,
) -> Result<(), CodecError> {
    writer.write_str(value.workspace_path())?;
    writer.write_collection_len(value.changed_paths().len())?;
    for path in value.changed_paths() {
        writer.write_str(path)?;
    }
    writer.write_collection_len(value.successful_commands().len())?;
    for command in value.successful_commands() {
        writer.write_str(command)?;
    }
    writer.write_str(value.run_instructions())?;
    writer.write_bool(value.accepted())?;
    writer.write_str(value.commit_revision())?;
    writer.write_str(value.export_path())?;
    writer.write_bool(value.discarded())
}

fn read_deliverable(reader: &mut CanonicalReader<'_>) -> Result<ProductDeliverable, CodecError> {
    let offset = reader.offset();
    let workspace_path = reader.read_str()?.to_owned();
    let path_count = reader.read_collection_len()?;
    if path_count > MAX_PRODUCT_DELIVERABLE_PATHS {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let changed_paths = (0..path_count)
        .map(|_| reader.read_str().map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;
    let command_count = reader.read_collection_len()?;
    if command_count > MAX_PRODUCT_DELIVERABLE_COMMANDS {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let successful_commands = (0..command_count)
        .map(|_| reader.read_str().map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;
    invalid(
        offset,
        ProductDeliverable::restore(
            workspace_path,
            changed_paths,
            successful_commands,
            reader.read_str()?.to_owned(),
            reader.read_bool()?,
            reader.read_str()?.to_owned(),
            reader.read_str()?.to_owned(),
            reader.read_bool()?,
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
