//! Strict decoding for application tables.

use peritus_types::{ActorId, ArtifactId, CommandId, SessionId, WorkspaceId};
use rusqlite::Row;

use super::types::{
    ApplicationArtifact, ApplicationArtifactState, ApplicationCommandRecord,
    ApplicationCommandState, ApplicationPrincipal, ApplicationPrincipalKind,
    ApplicationPrincipalState, ApplicationRequestId, ApplicationSession, ApplicationSessionState,
    ApplicationWorkspace, ApplicationWorkspaceState, MAX_APPLICATION_WORKSPACE_REGISTRATION_BYTES,
};
use crate::{
    JournalError,
    sqlite::query::{array_from_blob, corrupt, digest_from_blob, positive_u64},
};

pub(super) struct PrincipalRow {
    digest: Vec<u8>,
    kind: i64,
    actor: Vec<u8>,
    binding: Vec<u8>,
    state: i64,
}

impl PrincipalRow {
    pub(super) fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            digest: row.get(0)?,
            kind: row.get(1)?,
            actor: row.get(2)?,
            binding: row.get(3)?,
            state: row.get(4)?,
        })
    }

    pub(super) fn parse(self) -> Result<ApplicationPrincipal, JournalError> {
        Ok(ApplicationPrincipal {
            principal_digest: digest_from_blob(&self.digest, "application principal digest")?,
            kind: ApplicationPrincipalKind::from_tag(self.kind)
                .ok_or_else(|| corrupt("unknown application principal kind"))?,
            actor_id: actor_id(&self.actor)?,
            binding_digest: digest_from_blob(&self.binding, "application binding digest")?,
            state: ApplicationPrincipalState::from_tag(self.state)
                .ok_or_else(|| corrupt("unknown application principal state"))?,
        })
    }
}

pub(super) struct SessionRow {
    session: Vec<u8>,
    actor: Vec<u8>,
    authority_epoch: i64,
    state: i64,
    created_at: i64,
    protocol_id: Vec<u8>,
    version_major: i64,
    version_minor: i64,
}

impl SessionRow {
    pub(super) fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            session: row.get(0)?,
            actor: row.get(1)?,
            authority_epoch: row.get(2)?,
            state: row.get(3)?,
            created_at: row.get(4)?,
            protocol_id: row.get(5)?,
            version_major: row.get(6)?,
            version_minor: row.get(7)?,
        })
    }

    pub(super) fn parse(self) -> Result<ApplicationSession, JournalError> {
        Ok(ApplicationSession {
            session_id: session_id(&self.session)?,
            actor_id: actor_id(&self.actor)?,
            authority_epoch: positive_u64(self.authority_epoch, "application authority epoch")?,
            state: ApplicationSessionState::from_tag(self.state)
                .ok_or_else(|| corrupt("unknown application session state"))?,
            created_at: positive_u64(self.created_at, "application session creation tick")?,
            protocol_id: array_from_blob(&self.protocol_id, "application protocol identity")?,
            version_major: positive_u16(self.version_major, "application protocol major version")?,
            version_minor: nonnegative_u16(self.version_minor)?,
        })
    }
}

pub(super) struct CommandRow {
    actor: Vec<u8>,
    session: Vec<u8>,
    key: Vec<u8>,
    request_digest: Vec<u8>,
    domain_command_digest: Vec<u8>,
    request_id: Vec<u8>,
    command_id: Vec<u8>,
    state: i64,
    first: Option<i64>,
    last: Option<i64>,
    error_code: Option<String>,
    result_digest: Option<Vec<u8>>,
}

impl CommandRow {
    pub(super) fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            actor: row.get(0)?,
            session: row.get(1)?,
            key: row.get(2)?,
            request_digest: row.get(3)?,
            request_id: row.get(4)?,
            domain_command_digest: row.get(5)?,
            command_id: row.get(6)?,
            state: row.get(7)?,
            first: row.get(8)?,
            last: row.get(9)?,
            error_code: row.get(10)?,
            result_digest: row.get(11)?,
        })
    }

    pub(super) fn parse(self) -> Result<ApplicationCommandRecord, JournalError> {
        if self.key.is_empty() || self.key.len() > 256 {
            return Err(corrupt("stored application idempotency key is invalid"));
        }
        let state = ApplicationCommandState::from_tag(self.state)
            .ok_or_else(|| corrupt("unknown application command state"))?;
        let first_position = self
            .first
            .map(|value| positive_u64(value, "first application command position"))
            .transpose()?;
        let last_position = self
            .last
            .map(|value| positive_u64(value, "last application command position"))
            .transpose()?;
        let result_digest = self
            .result_digest
            .as_deref()
            .map(|bytes| digest_from_blob(bytes, "application result digest"))
            .transpose()?;
        let record = ApplicationCommandRecord {
            actor_id: actor_id(&self.actor)?,
            session_id: session_id(&self.session)?,
            idempotency_key: self.key,
            request_digest: digest_from_blob(&self.request_digest, "application request digest")?,
            domain_command_digest: digest_from_blob(
                &self.domain_command_digest,
                "application domain command digest",
            )?,
            request_id: ApplicationRequestId::new(array_from_blob(
                &self.request_id,
                "application request identity",
            )?)
            .map_err(|_| corrupt("stored application request identity is invalid"))?,
            command_id: CommandId::new(array_from_blob(
                &self.command_id,
                "application command identity",
            )?)
            .map_err(|_| corrupt("stored application command identity is invalid"))?,
            state,
            first_position,
            last_position,
            error_code: self.error_code,
            result_digest,
        };
        validate_command_shape(&record)?;
        Ok(record)
    }
}

pub(super) struct ArtifactRow {
    id: Vec<u8>,
    digest: Vec<u8>,
    size: i64,
    media_type: String,
    state: i64,
    producing_position: Option<i64>,
}

impl ArtifactRow {
    pub(super) fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            digest: row.get(1)?,
            size: row.get(2)?,
            media_type: row.get(3)?,
            state: row.get(4)?,
            producing_position: row.get(5)?,
        })
    }
    pub(super) fn parse(self) -> Result<ApplicationArtifact, JournalError> {
        let byte_size = u64::try_from(self.size)
            .map_err(|_| corrupt("stored application artifact size is negative"))?;
        if self.media_type.is_empty() || self.media_type.len() > 255 || !self.media_type.is_ascii()
        {
            return Err(corrupt("stored application artifact media type is invalid"));
        }
        Ok(ApplicationArtifact {
            artifact_id: ArtifactId::new(array_from_blob(
                &self.id,
                "application artifact identity",
            )?)
            .map_err(|_| corrupt("stored application artifact identity is invalid"))?,
            digest: digest_from_blob(&self.digest, "application artifact digest")?,
            byte_size,
            media_type: self.media_type,
            state: ApplicationArtifactState::from_tag(self.state)
                .ok_or_else(|| corrupt("unknown application artifact state"))?,
            producing_position: self
                .producing_position
                .map(|value| positive_u64(value, "application artifact position"))
                .transpose()?,
        })
    }
}

pub(super) struct WorkspaceRow {
    id: Vec<u8>,
    bytes: Vec<u8>,
    digest: Vec<u8>,
    state: i64,
}

impl WorkspaceRow {
    pub(super) fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self { id: row.get(0)?, bytes: row.get(1)?, digest: row.get(2)?, state: row.get(3)? })
    }
    pub(super) fn parse(self) -> Result<ApplicationWorkspace, JournalError> {
        if self.bytes.is_empty() || self.bytes.len() > MAX_APPLICATION_WORKSPACE_REGISTRATION_BYTES
        {
            return Err(corrupt("stored workspace registration is outside the production bound"));
        }
        let registration_digest = digest_from_blob(&self.digest, "application workspace digest")?;
        if peritus_codec::sha256(&self.bytes) != registration_digest {
            return Err(corrupt("stored workspace registration digest differs from its bytes"));
        }
        Ok(ApplicationWorkspace {
            workspace_id: WorkspaceId::new(array_from_blob(
                &self.id,
                "application workspace identity",
            )?)
            .map_err(|_| corrupt("stored application workspace identity is invalid"))?,
            registration_bytes: self.bytes,
            registration_digest,
            state: ApplicationWorkspaceState::from_tag(self.state)
                .ok_or_else(|| corrupt("unknown application workspace state"))?,
        })
    }
}

fn validate_command_shape(record: &ApplicationCommandRecord) -> Result<(), JournalError> {
    let valid = match record.state {
        ApplicationCommandState::Pending | ApplicationCommandState::Indeterminate => {
            record.first_position.is_none()
                && record.last_position.is_none()
                && record.error_code.is_none()
                && record.result_digest.is_none()
        }
        ApplicationCommandState::Committed => {
            record.first_position.is_some_and(|first| first > 0)
                && record
                    .last_position
                    .zip(record.first_position)
                    .is_some_and(|(last, first)| last >= first)
                && record.error_code.is_none()
                && record.result_digest.is_some()
        }
        ApplicationCommandState::Rejected => {
            record.first_position.is_none()
                && record.last_position.is_none()
                && record
                    .error_code
                    .as_ref()
                    .is_some_and(|code| !code.is_empty() && code.len() <= 128)
                && record.result_digest.is_some()
        }
    };
    if valid { Ok(()) } else { Err(corrupt("stored application command shape is inconsistent")) }
}

fn actor_id(bytes: &[u8]) -> Result<ActorId, JournalError> {
    ActorId::new(array_from_blob(bytes, "application actor identity")?)
        .map_err(|_| corrupt("stored application actor identity is invalid"))
}

fn session_id(bytes: &[u8]) -> Result<SessionId, JournalError> {
    SessionId::new(array_from_blob(bytes, "application session identity")?)
        .map_err(|_| corrupt("stored application session identity is invalid"))
}

fn positive_u16(value: i64, _field: &'static str) -> Result<u16, JournalError> {
    let value = u16::try_from(value).map_err(|_| corrupt("stored application u16 is invalid"))?;
    if value == 0 { Err(corrupt("stored application positive u16 is zero")) } else { Ok(value) }
}

fn nonnegative_u16(value: i64) -> Result<u16, JournalError> {
    u16::try_from(value).map_err(|_| corrupt("stored application u16 is invalid"))
}
