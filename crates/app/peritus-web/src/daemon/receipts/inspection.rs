//! Exact durable receipt inspection has no transmission fallback.
use super::{
    App, AppErrorCode, AppMessage, AppRequestPayload, AppResponsePayload, Result, STANDARD,
    WorkbenchCommand, problem, retain_reconciled_response,
};
use base64::Engine as _;
use crate::error::uncertain;

pub async fn inspect_workbench_command(
    app: &App, operation: &str, command: &WorkbenchCommand,
) -> Result<Option<AppResponsePayload>> {
    let key = format!("daemon:{operation}");
    let Some(record) = app.operation(&key)? else { return Ok(None) };
    let frame = STANDARD.decode(record.input["frame"].as_str()
        .ok_or_else(|| problem("Missing original native request"))?).map_err(problem)?;
    let AppMessage::Request(original) = peritus_app_protocol::decode_app_message(
        &frame, peritus_app_protocol::AppProtocolLimits::PRODUCTION,
    ).map_err(problem)? else { return Err(problem("Invalid retained native request")) };
    if original.payload() != &AppRequestPayload::WorkbenchCommand(command.clone()) {
        return Err(problem("The retained receipt belongs to different workbench input"));
    }
    if let Some(result) = record.result {
        let bytes = STANDARD.decode(result["frame"].as_str()
            .ok_or_else(|| problem("Missing original native receipt"))?).map_err(problem)?;
        let AppMessage::Response(response) = peritus_app_protocol::decode_app_message(
            &bytes, peritus_app_protocol::AppProtocolLimits::PRODUCTION,
        ).map_err(problem)? else { return Err(problem("Invalid retained native receipt")) };
        if response.context() != original.context()
            || response.request_id() != original.request_id()
            || response.correlation_id() != original.correlation_id()
        {
            return Err(problem("The retained receipt belongs to another native request"));
        }
        return match response.payload() {
            AppResponsePayload::WorkbenchReceipt(receipt)
                if receipt.operation() == command.operation() && receipt.query() == command.query() =>
                Ok(Some(response.payload().clone())),
            AppResponsePayload::Error(_) => Ok(Some(response.payload().clone())),
            _ => Err(problem("Unexpected retained workbench receipt")),
        };
    }
    let Some(owner) = super::owner::retained(&record.input, &original)? else { return Ok(None) };
    match super::super::raw_request_owned(app, &owner, AppRequestPayload::QueryWorkbenchReceipt(command.clone())).await? {
        AppResponsePayload::WorkbenchReceipt(receipt)
            if receipt.operation() == command.operation() && receipt.query() == command.query() => {
            let payload = AppResponsePayload::WorkbenchReceipt(receipt);
            let payload = retain_reconciled_response(app, &key, &original, payload).await?;
            Ok(Some(payload))
        }
        AppResponsePayload::Error(error) if error.code() == AppErrorCode::InvalidIdentifier => Ok(None),
        AppResponsePayload::Error(error) => Err(uncertain(format!("The original receipt cannot be inspected: {error}"))),
        _ => Err(uncertain("Unexpected response while inspecting the original receipt")),
    }
}
