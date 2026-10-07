//! Durable application and process recovery before intake.

use peritus_journal::{JournalError, SqliteJournal};
use peritus_process::{NativeProcessProbe, ProcessStore, RecoveryDisposition};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

pub(super) fn reconcile_application(journal: &mut SqliteJournal) -> Result<(), DaemonError> {
    let mut scheduler_session = peritus_scheduler::SchedulerSession::default();
    loop {
        let commands = journal.unsettled_application_commands(4_096).map_err(journal_error)?;
        if commands.is_empty() {
            return Ok(());
        }
        for command in commands {
            crate::command::reconcile_record(journal, &mut scheduler_session, command)?;
        }
    }
}

pub(super) fn reconcile_processes(store: &ProcessStore) -> Result<Option<String>, DaemonError> {
    let mut probe = NativeProcessProbe::new();
    let mut indeterminate = 0_usize;
    let mut scoped_failures = 0_usize;
    let mut first_scoped_failure = None;
    let mut count_overflow = false;
    let quarantined = store.reconcile_observations(&mut probe, |observation| {
        let (entry, failure) = observation.into_parts();
        if entry.disposition() == RecoveryDisposition::Indeterminate {
            match indeterminate.checked_add(1) {
                Some(next) => indeterminate = next,
                None => count_overflow = true,
            }
        }
        if let Some(error) = failure {
            match scoped_failures.checked_add(1) {
                Some(next) => scoped_failures = next,
                None => count_overflow = true,
            }
            if first_scoped_failure.is_none() {
                first_scoped_failure = Some(format!(
                    "process {:?}: {error}",
                    entry.process_id(),
                ));
            }
        }
        Ok(())
    }).map_err(|error| {
        DaemonError::with_source(
            DaemonErrorCode::RecoveryRequired,
            DaemonRecovery::Reconcile,
            "reconcile process registry",
            error.to_string(),
            error,
        )
    })?;
    if count_overflow {
        return Err(DaemonError::new(
            DaemonErrorCode::ResourceLimit,
            DaemonRecovery::Operator,
            "summarize process recovery",
            "process recovery record count overflowed",
        ));
    }
    let unresolved = indeterminate.checked_add(quarantined).ok_or_else(|| {
        DaemonError::new(
            DaemonErrorCode::ResourceLimit,
            DaemonRecovery::Operator,
            "summarize process recovery",
            "process recovery record count overflowed",
        )
    })?;
    if unresolved == 0 {
        Ok(None)
    } else {
        let scoped = first_scoped_failure.map_or_else(String::new, |failure| {
            format!(
                "; {scoped_failures} scoped native failures retained; first failure: {failure}"
            )
        });
        Ok(Some(format!(
            "PERITUS-STARTUP-RECOVERY-001: {indeterminate} indeterminate and {quarantined} quarantined process records require operator reconciliation{scoped}"
        )))
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
