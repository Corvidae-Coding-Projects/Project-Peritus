//! Shared canonical wire helpers for the three D0 families.

use super::AgentCountersDto;
use crate::primitive::{read_digest, write_digest};
use peritus_codec::{
    CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind, CodecLimits, sha256,
};
use peritus_types::Sha256Digest;

const PAYLOAD_DOMAIN: &[u8] = b"peritus.agent.inert-payload.v1\0";

pub(super) fn checked_payload(
    payload: Vec<u8>,
    limits: CodecLimits,
) -> Result<(Sha256Digest, Vec<u8>), CodecError> {
    let mut writer = CanonicalWriter::new(limits);
    writer.write_bytes(&payload)?;
    Ok((payload_digest(&payload), payload))
}

pub(super) fn payload_digest(payload: &[u8]) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(PAYLOAD_DOMAIN.len() + payload.len());
    bytes.extend_from_slice(PAYLOAD_DOMAIN);
    bytes.extend_from_slice(payload);
    sha256(&bytes)
}

pub(super) fn write_payload(
    writer: &mut CanonicalWriter,
    digest: Sha256Digest,
    payload: &[u8],
) -> Result<(), CodecError> {
    write_digest(writer, &digest)?;
    writer.write_bytes(payload)
}

pub(super) fn read_payload(
    reader: &mut CanonicalReader<'_>,
) -> Result<(Sha256Digest, Vec<u8>), CodecError> {
    let offset = reader.offset();
    let digest = read_digest(reader)?;
    let payload = reader.read_bytes_owned()?;
    if digest != payload_digest(&payload) {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    Ok((digest, payload))
}

pub(super) fn write_counters(
    writer: &mut CanonicalWriter,
    counters: AgentCountersDto,
) -> Result<(), CodecError> {
    writer.write_u64(counters.tool_calls())?;
    writer.write_u64(counters.provider_events())?;
    writer.write_u64(counters.context_cycles())?;
    writer.write_u64(counters.output_bytes())?;
    writer.write_u64(counters.tool_result_bytes())?;
    writer.write_u64(counters.concurrent_calls_high_water())?;
    writer.write_u64(counters.transitions())
}

pub(super) fn read_counters(
    reader: &mut CanonicalReader<'_>,
) -> Result<AgentCountersDto, CodecError> {
    Ok(AgentCountersDto::new(
        reader.read_u64()?,
        reader.read_u64()?,
        reader.read_u64()?,
        reader.read_u64()?,
        reader.read_u64()?,
        reader.read_u64()?,
        reader.read_u64()?,
    ))
}
