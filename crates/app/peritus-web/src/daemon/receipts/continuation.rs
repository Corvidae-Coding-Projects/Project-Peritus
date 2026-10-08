//! Exact continuation admission is separate from current run activity.
use super::{
    App, AppErrorCode, AppRequestEnvelope, AppRequestPayload, AppResponsePayload, NativeOwner,
    Result, WorkbenchCommand, problem, retransmit,
};
use crate::error::uncertain;
use crate::state::OperationOwner;
use peritus_app_protocol::{WorkbenchContinuationAdmissionState as State, WorkbenchIntent};
use base64::Engine as _;

pub async fn inspect_admission(
    app: &App, operation: &str, command: &WorkbenchCommand,
) -> Result<Option<State>> {
    let Some(record) = app.operation(&format!("daemon:{operation}"))?
        else { return Ok(None) };
    let frame = super::STANDARD.decode(record.input["frame"].as_str()
        .ok_or_else(|| problem("Missing original continuation request"))?).map_err(problem)?;
    let peritus_app_protocol::AppMessage::Request(original) =
        peritus_app_protocol::decode_app_message(&frame, peritus_app_protocol::AppProtocolLimits::PRODUCTION)
            .map_err(problem)? else { return Err(problem("Invalid original continuation request")) };
    if original.payload() != &AppRequestPayload::WorkbenchCommand(command.clone()) {
        return Err(problem("The original continuation has different durable input"));
    }
    let Some(owner) = super::owner::retained(&record.input, &original)? else { return Ok(None) };
    admission(app, &owner, command).await
}

pub(super) async fn admission(
    app: &App, owner: &NativeOwner, command: &WorkbenchCommand,
) -> Result<Option<State>> {
    let WorkbenchIntent::ContinueExecution(settings) = command.intent() else {
        return Err(problem("Continuation inspection requires the original continuation command"));
    };
    match super::super::raw_request_owned(app, owner,
        AppRequestPayload::QueryWorkbenchContinuationAdmission(command.clone()),
    ).await? {
        AppResponsePayload::WorkbenchContinuationAdmission(value)
            if value.operation() == command.operation() && value.query() == command.query()
                && value.run() == settings.run() => Ok(Some(value.state())),
        AppResponsePayload::Error(error) if error.code() == AppErrorCode::InvalidIdentifier => Ok(None),
        AppResponsePayload::Error(error) => Err(uncertain(format!("The original continuation admission cannot be inspected: {error}"))),
        _ => Err(uncertain("The daemon returned another continuation admission")),
    }
}

pub(super) async fn drive_admitted(
    app: &App, owner: &NativeOwner, record_owner: &OperationOwner,
    original: &AppRequestEnvelope,
    command: &WorkbenchCommand, response: AppResponsePayload,
) -> Result<AppResponsePayload> {
    if !matches!(command.intent(), WorkbenchIntent::ContinueExecution(_)) {
        return Ok(response);
    }
    if admission(app, owner, command).await? == Some(State::LaunchOwned) {
        return Ok(response);
    }
    // Only an explicit mutation/retry reaches this helper. Observation never closes the gap.
    let response = retransmit(app, owner, record_owner, original,
        AppRequestPayload::WorkbenchCommand(command.clone()),
    ).await?;
    if admission(app, owner, command).await? == Some(State::LaunchOwned) {
        Ok(response)
    } else {
        Err(uncertain("The original continuation remains durably accepted pending launch. Retry this exact operation to finish admission."))
    }
}
