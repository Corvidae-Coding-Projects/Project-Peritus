//! Exact redispatch and settlement of admitted application commands.

use peritus_app_protocol::{AppErrorCode, AppProtocolLimits, CommandSubmissionFrames};
use peritus_journal::{
    ApplicationCommandRecord, ApplicationCommandSettlement, ApplicationCommandState,
    CommandResolution, JournalError, JournalErrorKind, SqliteJournal,
};

use crate::domain::{DomainOutcome, DomainSubmission};
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

pub(crate) fn reconcile_record(
    journal: &mut SqliteJournal,
    scheduler_session: &mut peritus_scheduler::SchedulerSession,
    record: ApplicationCommandRecord,
) -> Result<ApplicationCommandRecord, DaemonError> {
    if matches!(
        record.state(),
        ApplicationCommandState::Committed | ApplicationCommandState::Rejected
    ) {
        return Ok(record);
    }
    let settlement = match journal
        .resolve_command(record.command_id(), record.domain_command_digest())
        .map_err(journal_error)?
    {
        CommandResolution::Committed(batch) => ApplicationCommandSettlement::committed(
            &batch,
            super::committed_result_digest(&batch),
        ),
        CommandResolution::DefinitelyAbsent => match retained_submission(&record)? {
            Some(submission) => match crate::domain::dispatch(journal, scheduler_session, submission)? {
                DomainOutcome::Committed(batch) => ApplicationCommandSettlement::committed(
                    &batch,
                    super::committed_result_digest(&batch),
                ),
                DomainOutcome::Rejected(code) => rejected(&record, code)?,
            },
            // Version-three admissions did not retain redispatch bytes. Preserve their accepted
            // decoder and former deterministic resolution without applying it to new commands.
            None => rejected(&record, AppErrorCode::UnsupportedFamily)?,
        },
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
        .settle_application_command(record.command_id(), record.request_digest(), settlement)
        .map_err(journal_error)
}

fn retained_submission(
    record: &ApplicationCommandRecord,
) -> Result<Option<DomainSubmission>, DaemonError> {
    let (Some(envelope), Some(command)) = (
        record.envelope_frame_bytes(),
        record.domain_command_frame_bytes(),
    ) else {
        if record.envelope_frame_bytes().is_some() || record.domain_command_frame_bytes().is_some() {
            return Err(corrupt("retained application command frames are partial"));
        }
        return Ok(None);
    };
    let frames = CommandSubmissionFrames::parse(
        envelope.to_vec(),
        command.to_vec(),
        AppProtocolLimits::PRODUCTION,
    )
    .map_err(|_| corrupt("retained application command frames are not canonical"))?;
    if frames.command_frame().digest() != record.domain_command_digest() {
        return Err(corrupt("retained application command digest changed"));
    }
    let envelope = frames.envelope().as_domain();
    if envelope.command_id() != record.command_id() {
        return Err(corrupt("retained application command identity changed"));
    }
    Ok(Some(DomainSubmission::new(
        record.command_id(),
        envelope.event_id(),
        envelope.expected_previous_event_id(),
        envelope.revision(),
        frames.command_frame().family(),
        frames.command_frame().bytes().to_vec(),
    )))
}

fn rejected(
    record: &ApplicationCommandRecord,
    code: AppErrorCode,
) -> Result<ApplicationCommandSettlement, DaemonError> {
    ApplicationCommandSettlement::rejected(
        code.as_str().to_owned(),
        super::rejection_result_digest(record.command_id(), record.request_digest(), code.as_str()),
    )
    .map_err(journal_error)
}

fn journal_error(error: JournalError) -> DaemonError {
    let (code, recovery) = match error.kind() {
        JournalErrorKind::InvalidInput
        | JournalErrorKind::EmptyBatch
        | JournalErrorKind::DuplicateIdentity
        | JournalErrorKind::NonCanonicalOrder => {
            (DaemonErrorCode::InvalidInput, DaemonRecovery::CorrectRequest)
        }
        JournalErrorKind::Busy | JournalErrorKind::Storage => {
            (DaemonErrorCode::Storage, DaemonRecovery::Retry)
        }
        JournalErrorKind::IndeterminateCommit => {
            (DaemonErrorCode::RecoveryRequired, DaemonRecovery::Reconcile)
        }
        JournalErrorKind::CorruptJournal | JournalErrorKind::UnsupportedSchema => {
            (DaemonErrorCode::CorruptState, DaemonRecovery::ReadOnly)
        }
        JournalErrorKind::SequenceOverflow
        | JournalErrorKind::StaleHead
        | JournalErrorKind::IdempotencyConflict
        | JournalErrorKind::MissingArtifact
        | JournalErrorKind::StaleAuthorityEpoch
        | JournalErrorKind::StaleRegistry
        | JournalErrorKind::ReadOnly
        | JournalErrorKind::NotFound => {
            (DaemonErrorCode::RecoveryRequired, DaemonRecovery::Reconcile)
        }
    };
    DaemonError::with_source(code, recovery, error.operation(), error.to_string(), error)
}

fn corrupt(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        "recover application command",
        detail,
    )
}
