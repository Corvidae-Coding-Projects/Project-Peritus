//! Canonical recovery-record codec.

use peritus_types::{ProcessId, Sha256Digest};

use crate::{
    MacosError, MacosErrorKind, MacosOperation, RecoveryAction,
    canonical::{Reader, Writer},
};

use super::{
    CHECKSUM_BYTES, CleanupProgress, LEGACY_VERSION, MAGIC, MacosRecoveryRecord,
    PROCESS_BIRTH_VERSION, RuntimeIdentity, VERSION,
};

impl MacosRecoveryRecord {
    /// Decodes, verifies, and migrates a supported runtime record.
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
        let mut reader = Reader::native(&input[..checksum_offset]);
        if reader.fixed::<8>()? != MAGIC {
            return Err(recovery_error("unknown runtime record magic or version"));
        }
        let version = reader.u16()?;
        if !matches!(version, LEGACY_VERSION | PROCESS_BIRTH_VERSION | VERSION) {
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
        let root_start_token = if version == LEGACY_VERSION {
            None
        } else {
            decode_nonzero_u64(&mut reader)?
        };
        let process_group = decode_nonzero_u32(&mut reader)?;
        let activated = reader.boolean()?;
        let cleanup = CleanupProgress::from_facts(
            reader.boolean()?,
            reader.boolean()?,
            reader.boolean()?,
            reader.boolean()?,
            reader.boolean()?,
        );
        let materialized_secret_files = if version == VERSION {
            let count = reader.native_count()?;
            let mut paths = Vec::new();
            for _ in 0..count {
                paths.try_reserve(1).map_err(|_| {
                    recovery_error("materialized secret cleanup paths cannot be represented")
                })?;
                paths.push(native_string(&mut reader)?);
            }
            paths
        } else {
            Vec::new()
        };
        reader.finish()?;
        let mut identity = RuntimeIdentity::new(
            process_id,
            preparation_digest,
            profile_digest,
            helper_digest,
            proxy_routing_digest,
            secret_binding_digest,
            root_pid,
            process_group,
        );
        identity.root_start_token = root_start_token;
        let mut record = Self {
            identity,
            activated,
            cleanup,
            materialized_secret_files,
            canonical: input.to_vec(),
            digest: peritus_codec::sha256(input),
        };
        record.refresh()?;
        Ok(record)
    }

    pub(super) fn refresh(&mut self) -> Result<(), MacosError> {
        let mut writer = Writer::native();
        writer.fixed(&MAGIC)?;
        writer.u16(VERSION)?;
        writer.fixed(self.identity.process_id.as_bytes())?;
        writer.fixed(self.identity.preparation_digest.as_bytes())?;
        writer.fixed(self.identity.profile_digest.as_bytes())?;
        writer.fixed(self.identity.helper_digest.as_bytes())?;
        encode_optional_digest(&mut writer, self.identity.proxy_routing_digest)?;
        encode_optional_digest(&mut writer, self.identity.secret_binding_digest)?;
        encode_nonzero_u32(&mut writer, self.identity.root_pid)?;
        encode_nonzero_u64(&mut writer, self.identity.root_start_token)?;
        encode_nonzero_u32(&mut writer, self.identity.process_group)?;
        writer.boolean(self.activated)?;
        writer.boolean(self.cleanup.helper_quiescent)?;
        writer.boolean(self.cleanup.profile_released)?;
        writer.boolean(self.cleanup.proxy_released)?;
        writer.boolean(self.cleanup.secrets_released)?;
        writer.boolean(self.cleanup.support_threads_joined)?;
        writer.native_count(self.materialized_secret_files.len())?;
        for path in &self.materialized_secret_files {
            writer.native_bytes(path.as_bytes())?;
        }
        let mut bytes = writer.finish();
        bytes.extend_from_slice(peritus_codec::sha256(&bytes).as_bytes());
        self.digest = peritus_codec::sha256(&bytes);
        self.canonical = bytes;
        Ok(())
    }
}

fn native_string(reader: &mut Reader<'_>) -> Result<String, MacosError> {
    let encoded = reader.native_bytes()?;
    let mut bytes = Vec::new();
    bytes.try_reserve(encoded.len()).map_err(|_| {
        recovery_error("materialized secret cleanup path cannot be represented")
    })?;
    bytes.extend_from_slice(encoded);
    String::from_utf8(bytes)
        .map_err(|_| recovery_error("materialized secret cleanup path is not UTF-8"))
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

fn encode_nonzero_u64(writer: &mut Writer, value: Option<u64>) -> Result<(), MacosError> {
    writer.boolean(value.is_some())?;
    if let Some(value) = value {
        if value == 0 {
            return Err(recovery_error("runtime process birth token is zero"));
        }
        writer.u64(value)?;
    }
    Ok(())
}

fn decode_nonzero_u64(reader: &mut Reader<'_>) -> Result<Option<u64>, MacosError> {
    if !reader.boolean()? {
        return Ok(None);
    }
    let value = reader.u64()?;
    if value == 0 {
        return Err(recovery_error("runtime process birth token is zero"));
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
