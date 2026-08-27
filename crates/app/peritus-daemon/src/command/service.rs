//! A3 command binding to the durable C0 application ledger.

use peritus_app_protocol::{
    AppErrorCode, AppProtocolError, CommandBinding, CommandResult, CommittedEventRange,
    EventCursor, RequestId,
};
use peritus_journal::{
    ApplicationCommandAdmission, ApplicationCommandRecord, ApplicationCommandSettlement,
    ApplicationCommandState, ApplicationRequestId, NewApplicationCommand,
};
use peritus_types::ActorId;

use super::rejection_result_digest;
use crate::domain::{DomainOutcome, DomainSubmission};
use crate::{AuthorityHandle, DaemonError, DaemonErrorCode, DaemonRecovery};

pub(crate) async fn submit(
    authority: &AuthorityHandle,
    actor_id: ActorId,
    binding: &CommandBinding,
) -> Result<CommandResult, DaemonError> {
    if binding.actor_id() != actor_id {
        return Err(DaemonError::new(
            DaemonErrorCode::Unauthorized,
            DaemonRecovery::CorrectRequest,
            "admit application command",
            "command actor does not match the authenticated peer",
        ));
    }
    let envelope = binding.frames().envelope().as_domain();
    let request_id = ApplicationRequestId::new(binding.request_id().into_bytes())
        .map_err(journal_value_error)?;
    let command = NewApplicationCommand::new(
        actor_id,
        binding.session_id(),
        binding.idempotency_key().as_bytes().to_vec(),
        binding.request_digest().as_sha256(),
        binding.frames().command_frame().digest(),
        request_id,
        envelope.command_id(),
    )
    .map_err(journal_value_error)?;

    match authority.admit_command(command).await? {
        ApplicationCommandAdmission::Conflict(_) => Ok(CommandResult::rejected(
            binding.request_id(),
            AppProtocolError::new(AppErrorCode::IdempotencyConflict, None),
        )),
        ApplicationCommandAdmission::Existing(record) => {
            let record = if matches!(
                record.state(),
                ApplicationCommandState::Pending | ApplicationCommandState::Indeterminate
            ) {
                authority
                    .reconcile_command(
                        record.command_id(),
                        record.request_digest(),
                        record.domain_command_digest(),
                    )
                    .await?
            } else {
                record
            };
            retained_result(&record, true)
        }
        ApplicationCommandAdmission::Inserted(record) => {
            let command_frame = binding.frames().command_frame();
            let submission = DomainSubmission::new(
                record.command_id(),
                envelope.event_id(),
                envelope.expected_previous_event_id(),
                envelope.revision(),
                command_frame.family(),
                command_frame.bytes().to_vec(),
            );
            let outcome = match authority.dispatch_command(submission).await {
                Ok(outcome) => outcome,
                Err(error) => {
                    let _ = authority
                        .settle_command(
                            record.command_id(),
                            record.request_digest(),
                            ApplicationCommandSettlement::indeterminate(),
                        )
                        .await;
                    return Err(error);
                }
            };
            let settlement = match outcome {
                DomainOutcome::Committed(batch) => ApplicationCommandSettlement::committed(
                    &batch,
                    super::committed_result_digest(&batch),
                ),
                DomainOutcome::Rejected(code) => {
                    let digest = rejection_result_digest(
                        record.command_id(),
                        record.request_digest(),
                        code.as_str(),
                    );
                    ApplicationCommandSettlement::rejected(code.as_str().to_owned(), digest)
                        .map_err(journal_value_error)?
                }
            };
            let settled = authority
                .settle_command(record.command_id(), record.request_digest(), settlement)
                .await?;
            retained_result(&settled, false)
        }
    }
}

fn retained_result(
    record: &ApplicationCommandRecord,
    replay: bool,
) -> Result<CommandResult, DaemonError> {
    let request_id = request_id_from(record);
    match record.state() {
        ApplicationCommandState::Committed => {
            let first = record.first_position().ok_or_else(missing_command_facts)?;
            let last = record.last_position().ok_or_else(missing_command_facts)?;
            let range = CommittedEventRange::new(EventCursor::new(first), EventCursor::new(last))
                .map_err(protocol_value_error)?;
            Ok(if replay {
                CommandResult::replayed(request_id, range)
            } else {
                CommandResult::committed(request_id, range)
            })
        }
        ApplicationCommandState::Rejected => {
            let code = record
                .error_code()
                .and_then(app_error_from_name)
                .ok_or_else(missing_command_facts)?;
            Ok(CommandResult::rejected(request_id, AppProtocolError::new(code, None)))
        }
        ApplicationCommandState::Pending | ApplicationCommandState::Indeterminate => {
            Err(missing_command_facts())
        }
    }
}

fn app_error_from_name(name: &str) -> Option<AppErrorCode> {
    (1..=u16::MAX).find_map(|tag| {
        let code = AppErrorCode::from_tag(tag)?;
        (code.as_str() == name).then_some(code)
    })
}

fn request_id_from(record: &ApplicationCommandRecord) -> RequestId {
    RequestId::new(*record.request_id().as_bytes())
        .expect("journal application request identity is nonzero")
}

fn missing_command_facts() -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::ReadOnly,
        "construct application command response",
        "terminal application command facts are missing or invalid",
    )
}

fn journal_value_error(error: peritus_journal::JournalError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        error.operation(),
        error.to_string(),
        error,
    )
}

fn protocol_value_error(error: AppProtocolError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::ReadOnly,
        "construct application command response",
        error.to_string(),
        error,
    )
}
