//! Durable command reconciliation retained behind the authority owner.

use peritus_journal::{ApplicationCommandRecord, SqliteJournal};
use peritus_types::{CommandId, Sha256Digest};

use super::error::journal_error;
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

pub(super) fn reconcile_command(
    journal: &mut SqliteJournal,
    scheduler_session: &mut peritus_scheduler::SchedulerSession,
    command_id: CommandId,
    request_digest: Sha256Digest,
    domain_command_digest: Sha256Digest,
) -> Result<ApplicationCommandRecord, DaemonError> {
    let record = journal
        .application_command(command_id)
        .map_err(journal_error)?
        .ok_or_else(|| {
            DaemonError::new(
                DaemonErrorCode::CorruptState,
                DaemonRecovery::Operator,
                "reconcile application command",
                "application command ledger row is missing",
            )
        })?;
    if record.request_digest() != request_digest
        || record.domain_command_digest() != domain_command_digest
    {
        return Err(DaemonError::new(
            DaemonErrorCode::CorruptState,
            DaemonRecovery::Operator,
            "reconcile application command",
            "application command reconciliation identity differs",
        ));
    }
    crate::command::reconcile_record(journal, scheduler_session, record)
}
