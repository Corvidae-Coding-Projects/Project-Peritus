//! Canonical bounded project-initialization discovery and proposal codec.

use super::primitive::{invalid, read_digest, unknown, write_digest};
use crate::{
    InitCommand, InitCommandKind, InitCommandVerification, InitDiscoveryRequest, InitFileMode,
    InitInstructionPatch, InitProposal, InitSourceKind, InitSourceObservation,
    MAX_INIT_COMMAND_ARGUMENTS, MAX_INIT_COMMANDS, MAX_INIT_DIFF_BYTES, MAX_INIT_INSTRUCTION_BYTES,
    MAX_INIT_SOURCES,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind, CodecLimits};

#[cfg(test)]
mod tests;

pub(super) fn write_discovery_request(
    writer: &mut CanonicalWriter,
    value: InitDiscoveryRequest,
) -> Result<(), CodecError> {
    super::workbench::write_query(writer, value.query())?;
    writer.write_u64(value.revision())
}

pub(super) fn read_discovery_request(
    reader: &mut CanonicalReader<'_>,
) -> Result<InitDiscoveryRequest, CodecError> {
    let offset = reader.offset();
    let query = super::workbench::read_query(reader)?;
    let revision = reader.read_u64()?;
    invalid(offset, InitDiscoveryRequest::new(query, revision))
}

pub(super) fn write_proposal(
    writer: &mut CanonicalWriter,
    value: &InitProposal,
) -> Result<(), CodecError> {
    super::workbench::write_query(writer, value.query())?;
    writer.write_u64(value.revision())?;
    write_digest(writer, value.folder_digest())?;
    writer.write_collection_len(value.sources().len())?;
    for source in value.sources() {
        write_source(writer, source)?;
    }
    write_patch(writer, value.patch())?;
    writer.write_collection_len(value.commands().len())?;
    for command in value.commands() {
        write_command(writer, command)?;
    }
    Ok(())
}

pub(super) fn read_proposal(reader: &mut CanonicalReader<'_>) -> Result<InitProposal, CodecError> {
    let offset = reader.offset();
    let query = super::workbench::read_query(reader)?;
    let revision = reader.read_u64()?;
    let folder_digest = read_digest(reader)?;
    let source_count = bounded_count(reader, MAX_INIT_SOURCES, offset)?;
    let mut sources = Vec::with_capacity(source_count);
    for _ in 0..source_count {
        sources.push(read_source(reader)?);
    }
    let patch = read_patch(reader)?;
    let command_count = bounded_count(reader, MAX_INIT_COMMANDS, offset)?;
    let mut commands = Vec::with_capacity(command_count);
    for _ in 0..command_count {
        commands.push(read_command(reader)?);
    }
    invalid(offset, InitProposal::new(query, revision, folder_digest, sources, patch, commands))
}

fn write_source(
    writer: &mut CanonicalWriter,
    value: &InitSourceObservation,
) -> Result<(), CodecError> {
    writer.write_str(value.path())?;
    writer.write_u16(match value.kind() {
        InitSourceKind::Manifest => 1,
        InitSourceKind::Documentation => 2,
        InitSourceKind::Instructions => 3,
        InitSourceKind::CommandConfig => 4,
    })?;
    write_digest(writer, value.digest())?;
    writer.write_u64(value.bytes())
}

fn read_source(reader: &mut CanonicalReader<'_>) -> Result<InitSourceObservation, CodecError> {
    let offset = reader.offset();
    let path = bounded_string(reader, 4096, offset)?;
    let kind = match reader.read_u16()? {
        1 => InitSourceKind::Manifest,
        2 => InitSourceKind::Documentation,
        3 => InitSourceKind::Instructions,
        4 => InitSourceKind::CommandConfig,
        _ => return unknown(offset),
    };
    let digest = read_digest(reader)?;
    let bytes = reader.read_u64()?;
    invalid(offset, InitSourceObservation::new(path, kind, digest, bytes))
}

fn write_patch(
    writer: &mut CanonicalWriter,
    value: &InitInstructionPatch,
) -> Result<(), CodecError> {
    writer.write_str(value.path())?;
    writer.write_option_tag(value.original_content().is_some())?;
    if let Some(original) = value.original_content() {
        writer.write_str(original)?;
    }
    writer.write_option_tag(value.precondition_digest().is_some())?;
    if let Some(digest) = value.precondition_digest() {
        write_digest(writer, digest)?;
    }
    writer.write_u64(value.precondition_bytes())?;
    writer.write_u16(match value.mode() {
        InitFileMode::Regular => 1,
        InitFileMode::Executable => 2,
    })?;
    writer.write_str(value.proposed_content())?;
    writer.write_str(value.diff())
}

fn read_patch(reader: &mut CanonicalReader<'_>) -> Result<InitInstructionPatch, CodecError> {
    let offset = reader.offset();
    let path = bounded_string(reader, 4096, offset)?;
    let original_content = if reader.read_option_tag()? {
        Some(bounded_string(reader, MAX_INIT_INSTRUCTION_BYTES, offset)?)
    } else {
        None
    };
    let precondition_digest =
        if reader.read_option_tag()? { Some(read_digest(reader)?) } else { None };
    let precondition_bytes = reader.read_u64()?;
    let mode = match reader.read_u16()? {
        1 => InitFileMode::Regular,
        2 => InitFileMode::Executable,
        _ => return unknown(offset),
    };
    let proposed_content = bounded_string(reader, MAX_INIT_INSTRUCTION_BYTES, offset)?;
    let diff = bounded_string(reader, MAX_INIT_DIFF_BYTES, offset)?;
    invalid(
        offset,
        InitInstructionPatch::new(
            path,
            original_content,
            precondition_digest,
            precondition_bytes,
            mode,
            proposed_content,
            diff,
        ),
    )
}

fn write_command(writer: &mut CanonicalWriter, value: &InitCommand) -> Result<(), CodecError> {
    writer.write_u16(match value.kind() {
        InitCommandKind::Build => 1,
        InitCommandKind::Test => 2,
        InitCommandKind::Lint => 3,
        InitCommandKind::Launch => 4,
    })?;
    writer.write_str(value.source())?;
    writer.write_str(value.executable())?;
    writer.write_collection_len(value.arguments().len())?;
    for argument in value.arguments() {
        writer.write_str(argument)?;
    }
    writer.write_u16(match value.verification() {
        InitCommandVerification::Unverified => 1,
    })
}

fn read_command(reader: &mut CanonicalReader<'_>) -> Result<InitCommand, CodecError> {
    let offset = reader.offset();
    let kind = match reader.read_u16()? {
        1 => InitCommandKind::Build,
        2 => InitCommandKind::Test,
        3 => InitCommandKind::Lint,
        4 => InitCommandKind::Launch,
        _ => return unknown(offset),
    };
    let source = bounded_string(reader, 4096, offset)?;
    let executable = bounded_string(reader, 128, offset)?;
    let argument_count = bounded_count(reader, MAX_INIT_COMMAND_ARGUMENTS, offset)?;
    let mut arguments = Vec::with_capacity(argument_count);
    for _ in 0..argument_count {
        arguments.push(bounded_string(reader, 1024, offset)?);
    }
    let verification = match reader.read_u16()? {
        1 => InitCommandVerification::Unverified,
        _ => return unknown(offset),
    };
    invalid(offset, InitCommand::new(kind, source, executable, arguments, verification))
}

fn bounded_count(
    reader: &mut CanonicalReader<'_>,
    maximum: usize,
    offset: usize,
) -> Result<usize, CodecError> {
    let count = reader.read_collection_len()?;
    if count > maximum {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    Ok(count)
}

fn bounded_string(
    reader: &mut CanonicalReader<'_>,
    maximum: usize,
    offset: usize,
) -> Result<String, CodecError> {
    let value = reader.read_str()?;
    if value.len() > maximum {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    Ok(value.to_owned())
}

impl InitProposal {
    /// Encodes the complete exact proposal for reviewed-consent archival.
    ///
    /// # Errors
    /// Rejects codec resource-limit failures.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CodecError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        write_proposal(&mut writer, self)?;
        Ok(writer.into_bytes())
    }

    /// Domain-separates and hashes the complete exact proposal; the digest itself grants no
    /// authority.
    ///
    /// # Errors
    /// Rejects codec resource-limit failures as stable application-protocol errors.
    pub fn fingerprint(&self) -> Result<peritus_types::Sha256Digest, crate::AppProtocolError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer
            .write_fixed(b"peritus-workbench-init-proposal-v1")
            .map_err(crate::AppProtocolError::from_codec)?;
        write_proposal(&mut writer, self).map_err(crate::AppProtocolError::from_codec)?;
        Ok(peritus_codec::sha256(writer.as_slice()))
    }
}
