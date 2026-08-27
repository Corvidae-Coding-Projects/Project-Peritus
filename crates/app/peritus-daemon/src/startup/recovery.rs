//! Durable application and process recovery before intake.

use peritus_journal::{
    ApplicationCommandSettlement, CommandResolution, JournalError, SqliteJournal,
};
use peritus_process::{NativeProcessProbe, ProcessStore, RecoveryDisposition};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

pub(super) fn reconcile_application(journal: &mut SqliteJournal) -> Result<(), DaemonError> {
    loop {
        let commands = journal.unsettled_application_commands(4_096).map_err(journal_error)?;
        if commands.is_empty() {
            return Ok(());
        }
        for command in commands {
            let resolution = journal
                .resolve_command(command.command_id(), command.domain_command_digest())
                .map_err(journal_error)?;
            match resolution {
                CommandResolution::DefinitelyAbsent => {
                    let code = peritus_app_protocol::AppErrorCode::UnsupportedFamily.as_str();
                    let digest = crate::command::rejection_result_digest(
                        command.command_id(),
                        command.request_digest(),
                        code,
                    );
                    let settlement =
                        ApplicationCommandSettlement::rejected(code.to_owned(), digest)
                            .map_err(journal_error)?;
                    journal
                        .settle_application_command(
                            command.command_id(),
                            command.request_digest(),
                            settlement,
                        )
                        .map_err(journal_error)?;
                }
                CommandResolution::Committed(batch) => {
                    let result_digest = crate::command::committed_result_digest(&batch);
                    let settlement = ApplicationCommandSettlement::committed(&batch, result_digest);
                    journal
                        .settle_application_command(
                            command.command_id(),
                            command.request_digest(),
                            settlement,
                        )
                        .map_err(journal_error)?;
                }
                CommandResolution::Conflict { .. } => {
                    return Err(DaemonError::new(
                        DaemonErrorCode::CorruptState,
                        DaemonRecovery::Operator,
                        "reconcile application command",
                        "application and journal request digests disagree for one command identity",
                    ));
                }
            }
        }
    }
}

pub(super) fn reconcile_processes(store: &ProcessStore) -> Result<Option<String>, DaemonError> {
    let mut probe = NativeProcessProbe::new();
    let report = store.reconcile(&mut probe).map_err(|error| {
        DaemonError::with_source(
            DaemonErrorCode::RecoveryRequired,
            DaemonRecovery::Reconcile,
            "reconcile process registry",
            error.to_string(),
            error,
        )
    })?;
    let unresolved = report
        .entries()
        .iter()
        .filter(|entry| entry.disposition() == RecoveryDisposition::Indeterminate)
        .count();
    if unresolved == 0 {
        Ok(None)
    } else {
        Ok(Some(format!("{unresolved} process recovery records remain indeterminate")))
    }
}

fn journal_error(error: JournalError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::RecoveryRequired,
        DaemonRecovery::Reconcile,
        error.operation(),
        error.to_string(),
        error,
    )
}
