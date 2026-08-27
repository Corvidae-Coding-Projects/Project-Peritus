//! Reconstruction of in-memory prompt state from the durable terminal record.

use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppRequestPayload, ApprovalAnswer, PromptAnswer,
    PromptAnswerPayload, PromptCancellation, PromptCorrelation, PromptKind, decode_app_message,
    decode_prompt_binding_value,
};
use peritus_journal::{
    ApplicationPromptRecord, ApplicationPromptSettlement, ApplicationPromptSettlementKind,
    ApplicationPromptTargetKind,
};

use super::errors::prompt_error;
use crate::prompt::{PreparedPromptRegistration, PromptBroker};
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery, PromptTerminalStatus};

pub(super) fn restore_record(
    prompts: &mut PromptBroker,
    record: &ApplicationPromptRecord,
) -> Result<PromptTerminalStatus, DaemonError> {
    let binding =
        decode_prompt_binding_value(record.binding_bytes(), AppProtocolLimits::PRODUCTION)
            .map_err(|_| corrupt_binding("durable prompt binding is not canonical"))?;
    validate_restored_correlation(record, binding.correlation())?;
    let expected_kind = match binding.kind() {
        PromptKind::UserInput => ApplicationPromptTargetKind::UserInput,
        PromptKind::Approval => ApplicationPromptTargetKind::Approval,
    };
    if record.target_kind() != expected_kind {
        return Err(corrupt_binding(
            "durable prompt binding kind differs from its registered target",
        ));
    }
    let prepared = prompts
        .prepare_durable_registration(binding, record.maximum_answer_bytes())
        .map_err(prompt_error)?;
    restore_registration(prompts, prepared, record)
}

pub(super) fn restore_registration(
    prompts: &mut PromptBroker,
    prepared: PreparedPromptRegistration,
    record: &ApplicationPromptRecord,
) -> Result<PromptTerminalStatus, DaemonError> {
    let restored = record
        .settlement()
        .map(|settlement| decode_durable_settlement(record, settlement))
        .transpose()?;
    prompts.commit_durable_registration(prepared).map_err(prompt_error)?;
    match restored {
        None => Ok(PromptTerminalStatus::AwaitingAnswer),
        Some(DurablePromptSettlement::Answer(answer)) => {
            prompts.restore_durable_answer(answer).map_err(prompt_error)
        }
        Some(DurablePromptSettlement::Cancellation(cancellation)) => {
            prompts.restore_durable_cancellation(cancellation).map_err(prompt_error)
        }
    }
}

enum DurablePromptSettlement {
    Answer(PromptAnswer),
    Cancellation(PromptCancellation),
}

fn decode_durable_settlement(
    record: &ApplicationPromptRecord,
    settlement: &ApplicationPromptSettlement,
) -> Result<DurablePromptSettlement, DaemonError> {
    let AppMessage::Request(request) =
        decode_app_message(settlement.bytes(), AppProtocolLimits::PRODUCTION)
            .map_err(|_| corrupt_settlement("durable prompt settlement is not canonical"))?
    else {
        return Err(corrupt_settlement("durable prompt settlement is not a request frame"));
    };
    if *settlement.request_id().as_bytes() != *request.request_id().as_bytes() {
        return Err(corrupt_settlement(
            "durable prompt settlement request identity differs from its frame",
        ));
    }
    match request.payload() {
        AppRequestPayload::AnswerPrompt(answer) => {
            validate_restored_answer(record, settlement, answer)?;
            Ok(DurablePromptSettlement::Answer(answer.clone()))
        }
        AppRequestPayload::CancelPrompt(cancellation)
            if settlement.kind() == ApplicationPromptSettlementKind::Cancellation =>
        {
            validate_restored_correlation(record, cancellation.correlation())?;
            Ok(DurablePromptSettlement::Cancellation(*cancellation))
        }
        _ => Err(corrupt_settlement(
            "durable prompt settlement kind differs from its canonical request",
        )),
    }
}

fn validate_restored_answer(
    record: &ApplicationPromptRecord,
    settlement: &ApplicationPromptSettlement,
    answer: &PromptAnswer,
) -> Result<(), DaemonError> {
    validate_restored_correlation(record, answer.correlation())?;
    let expected = match (record.target_kind(), answer.payload()) {
        (ApplicationPromptTargetKind::UserInput, PromptAnswerPayload::UserInput(_)) => {
            ApplicationPromptSettlementKind::UserInput
        }
        (
            ApplicationPromptTargetKind::Approval,
            PromptAnswerPayload::Approval { answer: ApprovalAnswer::SignedDecision(_), .. },
        ) => ApplicationPromptSettlementKind::Approval,
        (
            ApplicationPromptTargetKind::Approval,
            PromptAnswerPayload::Approval { answer: ApprovalAnswer::Cancel, .. },
        ) => ApplicationPromptSettlementKind::Cancellation,
        _ => {
            return Err(corrupt_settlement(
                "durable prompt answer kind differs from its registered target",
            ));
        }
    };
    if settlement.kind() != expected {
        return Err(corrupt_settlement(
            "durable prompt settlement classification differs from its answer",
        ));
    }
    Ok(())
}

fn validate_restored_correlation(
    record: &ApplicationPromptRecord,
    correlation: PromptCorrelation,
) -> Result<(), DaemonError> {
    if *record.prompt_id().as_bytes() != *correlation.prompt_id().as_bytes()
        || record.actor_id() != correlation.actor_id()
        || record.session_id() != correlation.session_id()
        || *record.originating_request_id().as_bytes()
            != *correlation.originating_request_id().as_bytes()
        || record.revision() != correlation.revision()
        || record.freshness_digest() != correlation.freshness_digest()
        || record.cancellation_generation() != correlation.cancellation_generation()
    {
        return Err(corrupt_settlement(
            "durable prompt settlement correlation differs from its target",
        ));
    }
    Ok(())
}

fn corrupt_settlement(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::ReadOnly,
        "restore durable prompt settlement",
        detail,
    )
}

fn corrupt_binding(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::ReadOnly,
        "restore durable prompt binding",
        detail,
    )
}
