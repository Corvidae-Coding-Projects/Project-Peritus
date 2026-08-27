//! Durable command reconciliation retained behind the authority owner.

use peritus_journal::{
    ApplicationCommandRecord, ApplicationCommandSettlement, CommandResolution, SqliteJournal,
};
use peritus_types::{CommandId, Sha256Digest};

use super::error::journal_error;
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

pub(super) fn reconcile_command(
    journal: &mut SqliteJournal,
    command_id: CommandId,
    request_digest: Sha256Digest,
    domain_command_digest: Sha256Digest,
) -> Result<ApplicationCommandRecord, DaemonError> {
    let settlement =
        match journal.resolve_command(command_id, domain_command_digest).map_err(journal_error)? {
            CommandResolution::DefinitelyAbsent => {
                let code = peritus_app_protocol::AppErrorCode::UnsupportedFamily.as_str();
                ApplicationCommandSettlement::rejected(
                    code.to_owned(),
                    crate::command::rejection_result_digest(command_id, request_digest, code),
                )
                .map_err(journal_error)?
            }
            CommandResolution::Committed(batch) => ApplicationCommandSettlement::committed(
                &batch,
                crate::command::committed_result_digest(&batch),
            ),
            CommandResolution::Conflict { .. } => {
                return Err(DaemonError::new(
                    DaemonErrorCode::CorruptState,
                    DaemonRecovery::Operator,
                    "reconcile application command",
                    "application and journal command digests disagree",
                ));
            }
        };
    journal
        .settle_application_command(command_id, request_digest, settlement)
        .map_err(journal_error)
}
