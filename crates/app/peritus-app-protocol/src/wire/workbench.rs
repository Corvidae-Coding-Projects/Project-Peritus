//! Exact additive typed control codecs; no raw JSON or prompt macro crosses A3.

mod intent;

use super::primitive::{invalid, read_digest, read_id, write_digest, write_id};
use crate::{
    ControlOperationId, ConversationId, ConversationTitle, WorkbenchCommand, WorkbenchQuery,
    WorkbenchReceipt, WorkbenchSnapshot,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::WorkspaceId;

pub(super) fn write_query(
    w: &mut CanonicalWriter,
    query: WorkbenchQuery,
) -> Result<(), CodecError> {
    write_id(w, query.conversation().as_bytes())?;
    write_id(w, query.workspace().as_bytes())
}

pub(super) fn read_query(r: &mut CanonicalReader<'_>) -> Result<WorkbenchQuery, CodecError> {
    Ok(WorkbenchQuery::new(read_id(r, ConversationId::new)?, read_id(r, WorkspaceId::new)?))
}

pub(super) fn write_command(
    w: &mut CanonicalWriter,
    command: &WorkbenchCommand,
) -> Result<(), CodecError> {
    write_id(w, command.operation().as_bytes())?;
    write_query(w, command.query())?;
    w.write_u64(command.expected_revision())?;
    w.write_u16(intent::tag(command.intent()))?;
    intent::write(w, command.intent())
}

pub(super) fn read_command(r: &mut CanonicalReader<'_>) -> Result<WorkbenchCommand, CodecError> {
    let operation = read_id(r, ControlOperationId::new)?;
    let query = read_query(r)?;
    let expected_revision = r.read_u64()?;
    let offset = r.offset();
    let tag = r.read_u16()?;
    let intent = intent::read(r, tag, offset)?;
    Ok(WorkbenchCommand::new(operation, query, expected_revision, intent))
}

pub(super) fn write_snapshot(
    w: &mut CanonicalWriter,
    snapshot: &WorkbenchSnapshot,
) -> Result<(), CodecError> {
    write_query(w, snapshot.query())?;
    w.write_u64(snapshot.revision())?;
    w.write_str(snapshot.title().as_str())?;
    w.write_bool(snapshot.pinned())?;
    w.write_bool(snapshot.archived())
}

pub(super) fn read_snapshot(r: &mut CanonicalReader<'_>) -> Result<WorkbenchSnapshot, CodecError> {
    let offset = r.offset();
    let query = read_query(r)?;
    let revision = r.read_u64()?;
    let title = read_title(r)?;
    let pinned = r.read_bool()?;
    let archived = r.read_bool()?;
    invalid(offset, WorkbenchSnapshot::new(query, revision, title, pinned, archived))
}

pub(super) fn write_receipt(
    w: &mut CanonicalWriter,
    receipt: &WorkbenchReceipt,
) -> Result<(), CodecError> {
    write_id(w, receipt.operation().as_bytes())?;
    write_query(w, receipt.query())?;
    w.write_u64(receipt.accepted_revision())?;
    write_digest(w, receipt.payload_digest())
}

pub(super) fn read_receipt(r: &mut CanonicalReader<'_>) -> Result<WorkbenchReceipt, CodecError> {
    let offset = r.offset();
    let operation = read_id(r, ControlOperationId::new)?;
    let query = read_query(r)?;
    let revision = r.read_u64()?;
    let digest = read_digest(r)?;
    invalid(offset, WorkbenchReceipt::new(operation, query, revision, digest))
}

pub(super) fn read_title(r: &mut CanonicalReader<'_>) -> Result<ConversationTitle, CodecError> {
    let offset = r.offset();
    let text = r.read_str()?;
    if text.len() > crate::MAX_CONVERSATION_TITLE_BYTES {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    invalid(offset, ConversationTitle::new(text.to_owned()))
}

pub(super) fn write_settings(
    w: &mut CanonicalWriter,
    value: &crate::WorkbenchExecutionSettings,
) -> Result<(), CodecError> {
    write_id(w, value.run().as_bytes())?;
    let providers = value.providers();
    for provider in [providers.writer(), providers.reviewer(), providers.fixer()] {
        write_id(w, provider.as_bytes())?;
    }
    w.write_u16(value.mode().tag())?;
    w.write_bool(value.models().has_effort())?;
    super::interaction::write_models(w, value.models())
}

pub(super) fn read_settings(
    r: &mut CanonicalReader<'_>,
) -> Result<crate::WorkbenchExecutionSettings, CodecError> {
    let run = read_id(r, peritus_types::RunId::new)?;
    let providers = crate::ProductProviderSelection::new(
        read_id(r, peritus_types::ProviderProfileId::new)?,
        read_id(r, peritus_types::ProviderProfileId::new)?,
        read_id(r, peritus_types::ProviderProfileId::new)?,
    );
    let offset = r.offset();
    let mode = crate::ProductInteractionMode::from_tag(r.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, offset))?;
    let efforts = r.read_bool()?;
    let models = super::interaction::read_models(r, efforts)?;
    Ok(crate::WorkbenchExecutionSettings::new(run, providers, mode, models))
}

impl crate::WorkbenchExecutionSettings {
    /// Computes the exact initial execution-selection fingerprint, without credentials or prompts.
    ///
    /// # Errors
    /// Rejects canonical encoding exceeding production bounds.
    pub fn fingerprint(&self) -> Result<peritus_types::Sha256Digest, CodecError> {
        let mut writer = CanonicalWriter::new(peritus_codec::CodecLimits::PRODUCTION);
        write_settings(&mut writer, self)?;
        Ok(peritus_codec::sha256(&writer.into_bytes()))
    }
}

impl WorkbenchCommand {
    /// Computes a canonical operation fingerprint without an application envelope or actor claim.
    ///
    /// # Errors
    /// Rejects values that exceed production codec bounds.
    pub fn fingerprint(&self) -> Result<peritus_types::Sha256Digest, CodecError> {
        let mut writer = CanonicalWriter::new(peritus_codec::CodecLimits::PRODUCTION);
        write_command(&mut writer, self)?;
        Ok(peritus_codec::sha256(&writer.into_bytes()))
    }
}
