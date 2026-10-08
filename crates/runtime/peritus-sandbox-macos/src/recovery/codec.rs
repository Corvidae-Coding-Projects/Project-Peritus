//! Canonical recovery-record codec.

use peritus_process::CancellationReason;
use peritus_types::{ProcessId, Sha256Digest};

use crate::{
    MacosError, MacosErrorKind, MacosOperation, RecoveryAction,
    canonical::{Reader, Writer},
};

use super::{
    CHECKSUM_BYTES, CUSTODY_VERSION, CleanupProgress, FILE_CLEANUP_VERSION, LEGACY_VERSION,
    MAGIC, MacosRecoveryRecord, PROCESS_BIRTH_VERSION, RecoveryResourceState, RuntimeIdentity,
    SessionCustody, VERSION,
};
use crate::{SessionPhase, TerminationReason};

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
        if !matches!(
            version,
            LEGACY_VERSION
                | PROCESS_BIRTH_VERSION
                | FILE_CLEANUP_VERSION
                | CUSTODY_VERSION
                | VERSION
        ) {
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
        let materialized_secret_files = if version >= FILE_CLEANUP_VERSION {
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
        let (phase, cancellation, termination, custody) = if version >= CUSTODY_VERSION {
            let phase = decode_phase(reader.u8()?)?;
            let cancellation = decode_cancellation(&mut reader)?;
            let termination = decode_termination(&mut reader)?;
            let custody = SessionCustody {
                owner_operation_digest: optional_digest(&mut reader)?,
                service_owner_digest: optional_digest(&mut reader)?,
                launch: decode_resource_state(reader.u8()?, version)?,
                execution_status: decode_resource_state(reader.u8()?, version)?,
                proxy: decode_resource_state(reader.u8()?, version)?,
                secrets: decode_resource_state(reader.u8()?, version)?,
                resource_monitor: decode_resource_state(reader.u8()?, version)?,
            };
            let unavailable_legacy_custody = custody.owner_operation_digest.is_none()
                && custody.service_owner_digest.is_none()
                && [
                    custody.launch,
                    custody.execution_status,
                    custody.proxy,
                    custody.secrets,
                    custody.resource_monitor,
                ]
                .into_iter()
                .all(|state| state != RecoveryResourceState::Live);
            let birth_identity_valid = match (
                identity_birth(root_pid, root_start_token, process_group),
                phase,
            ) {
                (BirthIdentity::Absent, SessionPhase::Prepared | SessionPhase::Released) => true,
                (BirthIdentity::Exact, _) => true,
                (BirthIdentity::Incomplete, _) if unavailable_legacy_custody => true,
                (BirthIdentity::Absent | BirthIdentity::Incomplete, _) => false,
            };
            let release_custody_valid =
                phase != SessionPhase::Released || custody.released();
            let proxy_cleanup_valid = if matches!(
                custody.proxy,
                RecoveryResourceState::CleanupRequired
            ) {
                version >= VERSION
                    && proxy_routing_digest.is_some()
                    && !cleanup.proxy_released()
                    && matches!(phase, SessionPhase::Prepared | SessionPhase::Terminated)
            } else {
                true
            };
            let cleanup_state_is_proxy_only = !matches!(
                custody.launch,
                RecoveryResourceState::CleanupRequired
            ) && !matches!(
                custody.execution_status,
                RecoveryResourceState::CleanupRequired
            ) && !matches!(
                custody.secrets,
                RecoveryResourceState::CleanupRequired
            ) && !matches!(
                custody.resource_monitor,
                RecoveryResourceState::CleanupRequired
            );
            if custody.owner_operation_digest.is_some()
                != custody.service_owner_digest.is_some()
                || phase == SessionPhase::Released
                    && (!cleanup.is_complete() || !materialized_secret_files.is_empty())
                || phase == SessionPhase::Cancelling && cancellation.is_none()
                || phase == SessionPhase::Terminated && termination.is_none()
                || phase == SessionPhase::Prepared && activated
                || matches!(phase, SessionPhase::Active | SessionPhase::Terminated) && !activated
                || !birth_identity_valid
                || !release_custody_valid
                || !proxy_cleanup_valid
                || !cleanup_state_is_proxy_only
            {
                return Err(recovery_error("runtime recovery custody is inconsistent"));
            }
            (phase, cancellation, termination, custody)
        } else {
            let legacy_file_cleanup_known =
                version >= FILE_CLEANUP_VERSION || secret_binding_digest.is_none();
            let phase = if cleanup.is_complete()
                && materialized_secret_files.is_empty()
                && legacy_file_cleanup_known
            {
                SessionPhase::Released
            } else if activated {
                SessionPhase::Active
            } else {
                SessionPhase::Prepared
            };
            (phase, None, None, SessionCustody::legacy(cleanup))
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
            phase,
            cancellation,
            termination,
            custody,
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
        writer.u8(phase_tag(self.phase))?;
        encode_cancellation(&mut writer, self.cancellation)?;
        encode_termination(&mut writer, self.termination)?;
        encode_optional_digest(&mut writer, self.custody.owner_operation_digest)?;
        encode_optional_digest(&mut writer, self.custody.service_owner_digest)?;
        for state in [
            self.custody.launch,
            self.custody.execution_status,
            self.custody.proxy,
            self.custody.secrets,
            self.custody.resource_monitor,
        ] {
            writer.u8(resource_state_tag(state))?;
        }
        let mut bytes = writer.finish();
        bytes.extend_from_slice(peritus_codec::sha256(&bytes).as_bytes());
        self.digest = peritus_codec::sha256(&bytes);
        self.canonical = bytes;
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum BirthIdentity {
    Absent,
    Exact,
    Incomplete,
}

const fn identity_birth(
    root_pid: Option<u32>,
    root_start_token: Option<u64>,
    process_group: Option<u32>,
) -> BirthIdentity {
    match (root_pid, root_start_token, process_group) {
        (None, None, None) => BirthIdentity::Absent,
        (Some(root), Some(_), Some(group)) if root == group => BirthIdentity::Exact,
        _ => BirthIdentity::Incomplete,
    }
}

const fn phase_tag(phase: SessionPhase) -> u8 {
    match phase {
        SessionPhase::Prepared => 1,
        SessionPhase::Active => 2,
        SessionPhase::Cancelling => 3,
        SessionPhase::Terminated => 4,
        SessionPhase::Released => 5,
    }
}

const fn decode_phase(tag: u8) -> Result<SessionPhase, MacosError> {
    match tag {
        1 => Ok(SessionPhase::Prepared),
        2 => Ok(SessionPhase::Active),
        3 => Ok(SessionPhase::Cancelling),
        4 => Ok(SessionPhase::Terminated),
        5 => Ok(SessionPhase::Released),
        _ => Err(recovery_error("runtime recovery phase is invalid")),
    }
}

fn encode_cancellation(
    writer: &mut Writer,
    reason: Option<CancellationReason>,
) -> Result<(), MacosError> {
    writer.u8(reason.map_or(0, cancellation_tag))
}

fn decode_cancellation(reader: &mut Reader<'_>) -> Result<Option<CancellationReason>, MacosError> {
    match reader.u8()? {
        0 => Ok(None),
        1 => Ok(Some(CancellationReason::User)),
        2 => Ok(Some(CancellationReason::Deadline)),
        3 => Ok(Some(CancellationReason::OutputLimit)),
        4 => Ok(Some(CancellationReason::ResourceLimit)),
        5 => Ok(Some(CancellationReason::LeaseFence)),
        6 => Ok(Some(CancellationReason::SupervisorShutdown)),
        7 => Ok(Some(CancellationReason::BackendFailure)),
        _ => Err(recovery_error("runtime recovery cancellation reason is invalid")),
    }
}

const fn cancellation_tag(reason: CancellationReason) -> u8 {
    match reason {
        CancellationReason::User => 1,
        CancellationReason::Deadline => 2,
        CancellationReason::OutputLimit => 3,
        CancellationReason::ResourceLimit => 4,
        CancellationReason::LeaseFence => 5,
        CancellationReason::SupervisorShutdown => 6,
        CancellationReason::BackendFailure => 7,
    }
}

fn encode_termination(
    writer: &mut Writer,
    termination: Option<TerminationReason>,
) -> Result<(), MacosError> {
    match termination {
        None => writer.u8(0),
        Some(TerminationReason::TargetExit(code)) => {
            writer.u8(1)?;
            writer.u32(u32::from_be_bytes(code.to_be_bytes()))
        }
        Some(TerminationReason::Signalled) => writer.u8(2),
        Some(TerminationReason::Unavailable) => writer.u8(3),
    }
}

fn decode_termination(reader: &mut Reader<'_>) -> Result<Option<TerminationReason>, MacosError> {
    match reader.u8()? {
        0 => Ok(None),
        1 => Ok(Some(TerminationReason::TargetExit(i32::from_be_bytes(
            reader.u32()?.to_be_bytes(),
        )))),
        2 => Ok(Some(TerminationReason::Signalled)),
        3 => Ok(Some(TerminationReason::Unavailable)),
        _ => Err(recovery_error("runtime recovery termination reason is invalid")),
    }
}

const fn resource_state_tag(state: RecoveryResourceState) -> u8 {
    match state {
        RecoveryResourceState::NotRequired => 1,
        RecoveryResourceState::Live => 2,
        RecoveryResourceState::Released => 3,
        RecoveryResourceState::Unavailable => 4,
        RecoveryResourceState::CleanupRequired => 5,
    }
}

const fn decode_resource_state(
    tag: u8,
    version: u16,
) -> Result<RecoveryResourceState, MacosError> {
    match tag {
        1 => Ok(RecoveryResourceState::NotRequired),
        2 => Ok(RecoveryResourceState::Live),
        3 => Ok(RecoveryResourceState::Released),
        4 => Ok(RecoveryResourceState::Unavailable),
        5 if version >= VERSION => Ok(RecoveryResourceState::CleanupRequired),
        _ => Err(recovery_error("runtime recovery resource custody is invalid")),
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
