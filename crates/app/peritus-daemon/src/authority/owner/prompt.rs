//! Lifecycle and ownership admission for prompt-broker operations.

mod errors;
mod restoration;

use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppRequestPayload, ApprovalAnswer, PromptAnswer,
    PromptAnswerPayload, PromptBinding, PromptCancellation, PromptCorrelation, PromptKind,
    RequestId, decode_app_message, encode_prompt_binding_value,
};
use peritus_approval::decode_credential_registry;
use peritus_journal::{
    ApplicationPromptId, ApplicationPromptRecord, ApplicationPromptRegistration,
    ApplicationPromptSettlement, ApplicationPromptSettlementKind, ApplicationPromptTargetKind,
    ApplicationRequestId, NewApplicationPromptTarget, SqliteJournal,
};
use peritus_types::{ActorId, SessionId};

use super::error::{journal_error, require_diagnostic, require_mutation};
use crate::{
    DaemonError, DaemonErrorCode, DaemonLifecycle, DaemonRecovery, PromptTerminalStatus,
    prompt::{
        AuthorityClock, CurrentApprovalAuthority, PromptAcceptance, PromptAdmission, PromptBroker,
    },
};
use errors::{
    approval_registry_error, missing_authority_epoch, ownership_error, prompt_error,
    prompt_mismatch, prompt_stale, protocol_encoding_error,
};
use restoration::{restore_record, restore_registration};

pub(super) fn register(
    journal: &mut SqliteJournal,
    prompts: &mut PromptBroker,
    lifecycle: &DaemonLifecycle,
    actor_id: ActorId,
    session_id: SessionId,
    binding: PromptBinding,
    maximum_answer_bytes: usize,
) -> Result<PromptTerminalStatus, DaemonError> {
    require_mutation(lifecycle)?;
    require_owner(actor_id, session_id, binding.correlation())?;
    let prepared = prompts
        .prepare_durable_registration(binding.clone(), maximum_answer_bytes)
        .map_err(prompt_error)?;
    let target = target(binding, maximum_answer_bytes)?;
    match journal.register_application_prompt_target(target).map_err(journal_error)? {
        ApplicationPromptRegistration::Inserted(record)
        | ApplicationPromptRegistration::Existing(record) => {
            restore_registration(prompts, prepared, &record)
        }
        ApplicationPromptRegistration::Conflict(_) => Err(DaemonError::new(
            DaemonErrorCode::RecoveryRequired,
            DaemonRecovery::Reconcile,
            "register prompt target",
            "prompt identity is already bound to different durable target facts",
        )),
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "prompt authority and exact wire facts remain explicit"
)]
pub(super) fn answer(
    journal: &mut SqliteJournal,
    prompts: &mut PromptBroker,
    clock: &AuthorityClock,
    lifecycle: &DaemonLifecycle,
    actor_id: ActorId,
    session_id: SessionId,
    request_id: RequestId,
    answer: PromptAnswer,
    request_frame: Vec<u8>,
) -> Result<PromptTerminalStatus, DaemonError> {
    require_mutation(lifecycle)?;
    validate_answer_frame(request_id, &answer, &request_frame)?;
    let target = load_target(journal, actor_id, session_id, answer.correlation())?;
    restore_record(prompts, &target)?;
    let admission = admission(&target);
    let prepared = if matches!(
        answer.payload(),
        PromptAnswerPayload::Approval { answer: ApprovalAnswer::SignedDecision(_), .. }
    ) {
        let epoch = journal
            .current_authority_epoch()
            .map_err(journal_error)?
            .ok_or_else(missing_authority_epoch)?;
        let current_registry = journal.current_credential_registry().map_err(journal_error)?;
        let registry =
            decode_credential_registry(current_registry.snapshot_payload().map_err(journal_error)?)
                .map_err(approval_registry_error)?;
        let observed_at = clock.now()?;
        prompts.prepare_answer(
            admission,
            answer,
            Some(CurrentApprovalAuthority::new(&epoch, &current_registry, &registry, observed_at)),
        )
    } else {
        prompts.prepare_answer(admission, answer, None)
    }
    .map_err(prompt_error)?;
    settle_prepared(
        journal,
        prompts,
        target,
        actor_id,
        session_id,
        request_id,
        request_frame,
        prepared,
    )
}

#[allow(
    clippy::too_many_arguments,
    reason = "prompt authority and exact wire facts remain explicit"
)]
pub(super) fn cancel(
    journal: &mut SqliteJournal,
    prompts: &mut PromptBroker,
    lifecycle: &DaemonLifecycle,
    actor_id: ActorId,
    session_id: SessionId,
    request_id: RequestId,
    cancellation: PromptCancellation,
    request_frame: Vec<u8>,
) -> Result<PromptTerminalStatus, DaemonError> {
    require_mutation(lifecycle)?;
    validate_cancellation_frame(request_id, cancellation, &request_frame)?;
    let target = load_target(journal, actor_id, session_id, cancellation.correlation())?;
    restore_record(prompts, &target)?;
    let prepared =
        prompts.prepare_cancel(admission(&target), cancellation).map_err(prompt_error)?;
    settle_prepared(
        journal,
        prompts,
        target,
        actor_id,
        session_id,
        request_id,
        request_frame,
        prepared,
    )
}

pub(super) fn status(
    prompts: &PromptBroker,
    lifecycle: &DaemonLifecycle,
    actor_id: ActorId,
    session_id: SessionId,
    correlation: PromptCorrelation,
) -> Result<PromptTerminalStatus, DaemonError> {
    require_diagnostic(lifecycle)?;
    require_owner(actor_id, session_id, correlation)?;
    prompts.status(correlation).map_err(prompt_error)
}

pub(super) fn retire(
    prompts: &mut PromptBroker,
    lifecycle: &DaemonLifecycle,
    actor_id: ActorId,
    session_id: SessionId,
    correlation: PromptCorrelation,
) -> Result<PromptTerminalStatus, DaemonError> {
    require_mutation(lifecycle)?;
    require_owner(actor_id, session_id, correlation)?;
    prompts.retire_terminal(correlation).map_err(prompt_error)
}

pub(super) fn correlations(
    prompts: &PromptBroker,
    lifecycle: &DaemonLifecycle,
    actor_id: ActorId,
    session_id: SessionId,
    maximum: usize,
) -> Result<Vec<PromptCorrelation>, DaemonError> {
    require_diagnostic(lifecycle)?;
    prompts.correlations_for(actor_id, session_id, maximum).map_err(prompt_error)
}

fn require_owner(
    actor_id: ActorId,
    session_id: SessionId,
    correlation: PromptCorrelation,
) -> Result<(), DaemonError> {
    if correlation.actor_id() != actor_id {
        return Err(ownership_error("prompt correlation belongs to another actor"));
    }
    if correlation.session_id() != session_id {
        return Err(ownership_error("prompt correlation belongs to another session"));
    }
    Ok(())
}

fn target(
    binding: PromptBinding,
    maximum_answer_bytes: usize,
) -> Result<NewApplicationPromptTarget, DaemonError> {
    let correlation = binding.correlation();
    let binding_bytes = encode_prompt_binding_value(&binding, AppProtocolLimits::PRODUCTION)
        .map_err(protocol_encoding_error)?;
    NewApplicationPromptTarget::new(
        application_prompt_id(correlation)?,
        correlation.actor_id(),
        correlation.session_id(),
        application_request_id(correlation.originating_request_id())?,
        match binding.kind() {
            PromptKind::UserInput => ApplicationPromptTargetKind::UserInput,
            PromptKind::Approval => ApplicationPromptTargetKind::Approval,
        },
        correlation.revision(),
        correlation.freshness_digest(),
        correlation.cancellation_generation(),
        peritus_codec::sha256(&binding_bytes),
        binding_bytes,
        maximum_answer_bytes,
    )
    .map_err(journal_error)
}

fn load_target(
    journal: &SqliteJournal,
    actor_id: ActorId,
    session_id: SessionId,
    correlation: PromptCorrelation,
) -> Result<ApplicationPromptRecord, DaemonError> {
    let target = journal
        .application_prompt_target(application_prompt_id(correlation)?)
        .map_err(journal_error)?
        .ok_or_else(|| prompt_mismatch("prompt target is not durably registered"))?;
    if target.actor_id() != actor_id || target.session_id() != session_id {
        return Err(ownership_error("prompt target belongs to another actor or session"));
    }
    if *target.originating_request_id().as_bytes()
        != *correlation.originating_request_id().as_bytes()
        || target.revision() != correlation.revision()
        || target.freshness_digest() != correlation.freshness_digest()
        || target.cancellation_generation() != correlation.cancellation_generation()
    {
        return Err(prompt_stale("prompt correlation differs from its current durable target"));
    }
    Ok(target)
}

const fn admission(target: &ApplicationPromptRecord) -> PromptAdmission {
    PromptAdmission::new(
        target.actor_id(),
        target.session_id(),
        target.revision(),
        target.cancellation_generation(),
    )
}

#[allow(clippy::too_many_arguments, reason = "durable prompt settlement inputs remain explicit")]
fn settle_prepared(
    journal: &mut SqliteJournal,
    prompts: &mut PromptBroker,
    target: ApplicationPromptRecord,
    actor_id: ActorId,
    session_id: SessionId,
    request_id: RequestId,
    request_frame: Vec<u8>,
    prepared: crate::prompt::PreparedPromptResponse,
) -> Result<PromptTerminalStatus, DaemonError> {
    let (acceptance, token) = prepared.into_parts();
    let kind = match acceptance {
        PromptAcceptance::UserInput(_) => ApplicationPromptSettlementKind::UserInput,
        PromptAcceptance::Approval(_) => ApplicationPromptSettlementKind::Approval,
        PromptAcceptance::Cancelled(_) => ApplicationPromptSettlementKind::Cancellation,
    };
    let digest = peritus_codec::sha256(&request_frame);
    let settlement = ApplicationPromptSettlement::new(
        kind,
        application_request_id(request_id)?,
        digest,
        request_frame,
    )
    .map_err(journal_error)?;
    journal
        .settle_application_prompt(
            target.prompt_id(),
            actor_id,
            session_id,
            target.binding_digest(),
            settlement,
        )
        .map_err(journal_error)?;
    prompts.commit_settlement(token).map_err(prompt_error)
}

fn validate_answer_frame(
    request_id: RequestId,
    expected: &PromptAnswer,
    frame: &[u8],
) -> Result<(), DaemonError> {
    match decode_app_message(frame, AppProtocolLimits::PRODUCTION) {
        Ok(AppMessage::Request(request))
            if request.request_id() == request_id
                && matches!(request.payload(), AppRequestPayload::AnswerPrompt(value) if value == expected) =>
        {
            Ok(())
        }
        _ => {
            Err(prompt_mismatch("canonical prompt answer frame differs from the authority request"))
        }
    }
}

fn validate_cancellation_frame(
    request_id: RequestId,
    expected: PromptCancellation,
    frame: &[u8],
) -> Result<(), DaemonError> {
    match decode_app_message(frame, AppProtocolLimits::PRODUCTION) {
        Ok(AppMessage::Request(request))
            if request.request_id() == request_id
                && matches!(request.payload(), AppRequestPayload::CancelPrompt(value) if *value == expected) =>
        {
            Ok(())
        }
        _ => Err(prompt_mismatch(
            "canonical prompt cancellation frame differs from the authority request",
        )),
    }
}

fn application_prompt_id(
    correlation: PromptCorrelation,
) -> Result<ApplicationPromptId, DaemonError> {
    ApplicationPromptId::new(*correlation.prompt_id().as_bytes()).map_err(journal_error)
}

fn application_request_id(request_id: RequestId) -> Result<ApplicationRequestId, DaemonError> {
    ApplicationRequestId::new(*request_id.as_bytes()).map_err(journal_error)
}
