//! Shared query and error helpers for application-ledger stores.

use peritus_types::{ActorId, ArtifactId, CommandId, SessionId, Sha256Digest};
use rusqlite::{Connection, OptionalExtension, params};

use super::{
    rows::{ArtifactRow, CommandRow, PrincipalRow, SessionRow},
    types::{
        ApplicationArtifact, ApplicationCommandRecord, ApplicationCommandSettlement,
        ApplicationCommandState, ApplicationPrincipal, ApplicationSession, SettlementKind,
    },
};
use crate::{JournalError, JournalErrorKind};

pub(super) const PRINCIPAL_COLUMNS: &str =
    "principal_digest, principal_kind, actor_id, binding_digest, state";
pub(super) const SESSION_COLUMNS: &str = "session_id, actor_id, authority_epoch, state, created_at, \
    last_protocol_id, last_version_major, last_version_minor";
pub(super) const COMMAND_COLUMNS: &str = "actor_id, session_id, idempotency_key, request_digest, request_id, \
    domain_command_digest, command_id, state, first_position, last_position, error_code, result_digest";
pub(super) const ARTIFACT_COLUMNS: &str =
    "artifact_id, digest, byte_size, media_type, state, producing_position";

pub(super) fn load_principal_by_digest(
    connection: &Connection,
    digest: Sha256Digest,
) -> Result<Option<ApplicationPrincipal>, JournalError> {
    let sql = format!("SELECT {PRINCIPAL_COLUMNS} FROM app_principals WHERE principal_digest = ?1");
    connection
        .query_row(&sql, params![digest.as_bytes().as_slice()], PrincipalRow::read)
        .optional()
        .map_err(|error| JournalError::sqlite("read application principal", error))?
        .map(PrincipalRow::parse)
        .transpose()
}

pub(super) fn load_principal_by_actor(
    connection: &Connection,
    actor: ActorId,
) -> Result<Option<ApplicationPrincipal>, JournalError> {
    let sql = format!("SELECT {PRINCIPAL_COLUMNS} FROM app_principals WHERE actor_id = ?1");
    connection
        .query_row(&sql, params![actor.as_bytes().as_slice()], PrincipalRow::read)
        .optional()
        .map_err(|error| JournalError::sqlite("read application principal actor", error))?
        .map(PrincipalRow::parse)
        .transpose()
}

pub(super) fn load_session(
    connection: &Connection,
    session: SessionId,
) -> Result<Option<ApplicationSession>, JournalError> {
    let sql = format!("SELECT {SESSION_COLUMNS} FROM app_sessions WHERE session_id = ?1");
    connection
        .query_row(&sql, params![session.as_bytes().as_slice()], SessionRow::read)
        .optional()
        .map_err(|error| JournalError::sqlite("read application session", error))?
        .map(SessionRow::parse)
        .transpose()
}

pub(super) fn load_command_by_key(
    connection: &Connection,
    actor: ActorId,
    session: SessionId,
    key: &[u8],
) -> Result<Option<ApplicationCommandRecord>, JournalError> {
    let sql = format!(
        "SELECT {COMMAND_COLUMNS} FROM app_commands WHERE actor_id = ?1 AND session_id = ?2 AND idempotency_key = ?3"
    );
    connection
        .query_row(
            &sql,
            params![actor.as_bytes().as_slice(), session.as_bytes().as_slice(), key],
            CommandRow::read,
        )
        .optional()
        .map_err(|error| JournalError::sqlite("read application command key", error))?
        .map(CommandRow::parse)
        .transpose()
}

pub(super) fn load_command_by_id(
    connection: &Connection,
    command: CommandId,
) -> Result<Option<ApplicationCommandRecord>, JournalError> {
    let sql = format!("SELECT {COMMAND_COLUMNS} FROM app_commands WHERE command_id = ?1");
    connection
        .query_row(&sql, params![command.as_bytes().as_slice()], CommandRow::read)
        .optional()
        .map_err(|error| JournalError::sqlite("read application command identity", error))?
        .map(CommandRow::parse)
        .transpose()
}

pub(super) fn load_artifact(
    connection: &Connection,
    artifact: ArtifactId,
) -> Result<Option<ApplicationArtifact>, JournalError> {
    let sql = format!("SELECT {ARTIFACT_COLUMNS} FROM app_artifacts WHERE artifact_id = ?1");
    connection
        .query_row(&sql, params![artifact.as_bytes().as_slice()], ArtifactRow::read)
        .optional()
        .map_err(|error| JournalError::sqlite("read application artifact", error))?
        .map(ArtifactRow::parse)
        .transpose()
}

pub(super) fn settlement_matches(
    record: &ApplicationCommandRecord,
    settlement: &ApplicationCommandSettlement,
) -> bool {
    match (&settlement.kind, record.state()) {
        (
            SettlementKind::Committed { first_position, last_position, result_digest },
            ApplicationCommandState::Committed,
        ) => {
            record.first_position() == Some(*first_position)
                && record.last_position() == Some(*last_position)
                && record.result_digest() == Some(*result_digest)
        }
        (
            SettlementKind::Rejected { error_code, result_digest },
            ApplicationCommandState::Rejected,
        ) => {
            record.error_code() == Some(error_code)
                && record.result_digest() == Some(*result_digest)
        }
        _ => false,
    }
}

pub(super) fn to_i64(value: u64, detail: &'static str) -> Result<i64, JournalError> {
    i64::try_from(value).map_err(|_| {
        JournalError::new(JournalErrorKind::SequenceOverflow, "write application ledger", detail)
    })
}

pub(super) const fn invalid(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::InvalidInput, "operate application ledger", detail)
}

pub(super) const fn conflict(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::IdempotencyConflict, "operate application ledger", detail)
}

pub(super) const fn not_found(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::NotFound, "operate application ledger", detail)
}

pub(super) const fn corrupt(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::CorruptJournal, "operate application ledger", detail)
}
