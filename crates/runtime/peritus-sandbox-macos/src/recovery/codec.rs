//! Canonical recovery-record codec.

use peritus_types::{ProcessId, Sha256Digest};

use crate::{
    MacosError, MacosErrorKind, MacosOperation, RecoveryAction,
    canonical::{Reader, Writer},
};

use super::{
    CHECKSUM_BYTES, CleanupProgress, MAGIC, MacosRecoveryRecord, RuntimeIdentity, VERSION,
};

impl MacosRecoveryRecord {
    /// Decodes and verifies a version-one runtime record.
    ///
    /// # Errors
    /// Returns a recovery-indeterminate error for malformed or checksummed data.
    pub fn decode(input: &[u8]) -> Result<Self, MacosError> {
        if input.len() < MAGIC.len() + 2 + CHECKSUM_BYTES {
            return Err(recovery_error("runtime record is truncated"));
        }
        let checksum_offset = input.len() - CHECKSUM_BYTES;
        if peritus_codec::sha256(&input[..checksum_offset]).as_bytes() != &input[checksum_offset..]
        {
            return Err(recovery_error("runtime record checksum does not match"));
        }
        let mut reader = Reader::new(&input[..checksum_offset])?;
        if reader.fixed::<8>()? != MAGIC || reader.u16()? != VERSION {
            return Err(recovery_error("unknown runtime record magic or version"));
        }
        let process_id = ProcessId::new(reader.fixed()?)
            .map_err(|_| recovery_error("runtime process identity is zero"))?;
        let preparation_digest = Sha256Digest::new(reader.fixed()?);
        let profile_digest = Sha256Digest::new(reader.fixed()?);
        let helper_digest = Sha256Digest::new(reader.fixed()?);
        let proxy_routing_digest = optional_digest(&mut reader)?;
        let secret_binding_digest = optional_digest(&mut reader)?;
        let root_pid = decode_nonzero_u32(&mut reader)?;
        let process_group = decode_nonzero_u32(&mut reader)?;
        let activated = reader.boolean()?;
        let cleanup = CleanupProgress::from_facts(
            reader.boolean()?,
            reader.boolean()?,
            reader.boolean()?,
            reader.boolean()?,
            reader.boolean()?,
        );
        reader.finish()?;
        let identity = RuntimeIdentity::new(
            process_id,
            preparation_digest,
            profile_digest,
            helper_digest,
            proxy_routing_digest,
            secret_binding_digest,
            root_pid,
            process_group,
        );
        let mut record = Self {
            identity,
            activated,
            cleanup,
            canonical: input.to_vec(),
            digest: peritus_codec::sha256(input),
        };
        let expected = record.canonical.clone();
        record.refresh()?;
        if record.canonical != expected {
            return Err(recovery_error("runtime record is not canonical"));
        }
        Ok(record)
    }

    pub(super) fn refresh(&mut self) -> Result<(), MacosError> {
        let mut writer = Writer::new();
        writer.fixed(&MAGIC)?;
        writer.u16(VERSION)?;
        writer.fixed(self.identity.process_id.as_bytes())?;
        writer.fixed(self.identity.preparation_digest.as_bytes())?;
        writer.fixed(self.identity.profile_digest.as_bytes())?;
        writer.fixed(self.identity.helper_digest.as_bytes())?;
        encode_optional_digest(&mut writer, self.identity.proxy_routing_digest)?;
        encode_optional_digest(&mut writer, self.identity.secret_binding_digest)?;
        encode_nonzero_u32(&mut writer, self.identity.root_pid)?;
        encode_nonzero_u32(&mut writer, self.identity.process_group)?;
        writer.boolean(self.activated)?;
        writer.boolean(self.cleanup.helper_quiescent)?;
        writer.boolean(self.cleanup.profile_released)?;
        writer.boolean(self.cleanup.proxy_released)?;
        writer.boolean(self.cleanup.secrets_released)?;
        writer.boolean(self.cleanup.support_threads_joined)?;
        let mut bytes = writer.finish();
        bytes.extend_from_slice(peritus_codec::sha256(&bytes).as_bytes());
        self.digest = peritus_codec::sha256(&bytes);
        self.canonical = bytes;
        Ok(())
    }
}

fn encode_optional_digest(
    writer: &mut Writer,
    value: Option<Sha256Digest>,
) -> Result<(), MacosError> {
    writer.boolean(value.is_some())?;
    if let Some(value) = value {
        writer.fixed(value.as_bytes())?;
    }
    Ok(())
}

fn optional_digest(reader: &mut Reader<'_>) -> Result<Option<Sha256Digest>, MacosError> {
    if reader.boolean()? { Ok(Some(Sha256Digest::new(reader.fixed()?))) } else { Ok(None) }
}

fn encode_nonzero_u32(writer: &mut Writer, value: Option<u32>) -> Result<(), MacosError> {
    writer.boolean(value.is_some())?;
    if let Some(value) = value {
        if value == 0 {
            return Err(recovery_error("runtime PID or process group is zero"));
        }
        writer.u32(value)?;
    }
    Ok(())
}

fn decode_nonzero_u32(reader: &mut Reader<'_>) -> Result<Option<u32>, MacosError> {
    if !reader.boolean()? {
        return Ok(None);
    }
    let value = reader.u32()?;
    if value == 0 {
        return Err(recovery_error("runtime PID or process group is zero"));
    }
    Ok(Some(value))
}

fn recovery_error(detail: &'static str) -> MacosError {
    MacosError::new(
        MacosErrorKind::RecoveryIndeterminate,
        MacosOperation::Recover,
        RecoveryAction::Quarantine,
        detail,
    )
}
