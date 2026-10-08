//! Retain exact native requests before transmission. Never infer acceptance from connectivity.
use super::{
    App, AppRequestPayload, AppResponsePayload, Client, NativeOwner, Result, Value, json, problem,
    response,
};
use crate::error::uncertain;
use crate::state::{OperationOwner, Publication};
use base64::{Engine, engine::general_purpose::STANDARD};
use peritus_app_protocol::{
    AppErrorCode, AppMessage, AppRequestEnvelope, WellKnownProtocolFeature, WorkbenchCommand,
    encode_app_message,
};
mod inspection;
mod owner;
mod continuation;
mod reconciliation;
pub use inspection::inspect_workbench_command;
pub use continuation::inspect_admission as inspect_continuation_admission;
pub use reconciliation::observed;
use reconciliation::recovery_baseline;

pub async fn recorded(
    app: &App,
    owner: &NativeOwner,
    operation: &str,
    payload: AppRequestPayload,
) -> Result<AppResponsePayload> {
    let key = format!("daemon:{operation}");
    let record_owner = app.own_operation(&key).await?;
    recorded_owned(app, owner, operation, payload, &record_owner).await
}

async fn recorded_owned(
    app: &App,
    owner: &NativeOwner,
    operation: &str,
    payload: AppRequestPayload,
    record_owner: &OperationOwner,
) -> Result<AppResponsePayload> {
    if record_owner.get()?.is_some() {
        return Err(uncertain(
            "The original native request was already submitted. Inspect the conversation and its operation record; it will not be sent again.",
        ));
    }
    let baseline = recovery_baseline(app, owner, &payload).await?;
    let required = required_features(&payload);
    let mut client = super::connect_owned(app, owner, &required).await?;
    let identity = Client::new_request_identity().map_err(problem)?;
    let envelope = AppRequestEnvelope::new(
        client.context(),
        identity.request_id,
        identity.correlation_id,
        payload.clone(),
    )
    .map_err(problem)?;
    let frame = STANDARD.encode(
        encode_app_message(&AppMessage::Request(envelope), client.limits()).map_err(problem)?,
    );
    if record_owner.insert(
        json!({"command":"daemon-request","parent":operation,"frame":frame,"baseline":baseline,"owner":owner}),
    )? == Publication::Existing {
        return Err(uncertain(
            "The original native request was concurrently submitted. Inspect its retained operation record; it will not be sent again.",
        ));
    }
    let response = client.request(identity, payload).await.map_err(|e| uncertain(e.to_string()))?;
    if let AppResponsePayload::Error(error) = response.payload()
        && matches!(
            error.code(),
            AppErrorCode::Internal | AppErrorCode::Backpressure | AppErrorCode::NotReady
        )
    {
        return Err(uncertain(format!("Daemon reported an indeterminate outcome: {error}")));
    }
    let result = json!({"frame":STANDARD.encode(encode_app_message(&AppMessage::Response(response.clone()),client.limits()).map_err(problem)?)});
    record_owner.settle(result).map_err(|error| uncertain(error.0))?;
    if let AppResponsePayload::Error(error) = response.payload() {
        return Err(problem(format!("Daemon rejected the request: {error}")));
    }
    Ok(response.payload().clone())
}

/// Sends one domain-idempotent workbench command or reconciles its original durable operation.
/// A missing authoritative receipt permits retransmission only with the same command identity.
pub async fn workbench_command(
    app: &App,
    owner: &NativeOwner,
    operation: &str,
    command: WorkbenchCommand,
) -> Result<AppResponsePayload> {
    let key = format!("daemon:{operation}");
    let payload = AppRequestPayload::WorkbenchCommand(command.clone());
    let record_owner = app.own_operation(&key).await?;
    let record = if let Some(record) = record_owner.get()? {
        record
    } else {
        recorded_owned(app, owner, operation, payload.clone(), &record_owner).await?;
        record_owner.get()?
            .ok_or_else(|| uncertain("The accepted native request record is unavailable"))?
    };
    let frame = STANDARD
        .decode(record.input["frame"].as_str().ok_or_else(|| problem("Missing daemon request"))?)
        .map_err(problem)?;
    let AppMessage::Request(original) = peritus_app_protocol::decode_app_message(
        &frame,
        peritus_app_protocol::AppProtocolLimits::PRODUCTION,
    )
    .map_err(problem)?
    else {
        return Err(problem("Invalid retained daemon request"));
    };
    if original.payload() != &payload {
        return Err(problem("The workbench operation identity was reused for different input"));
    }
    if record.input.get("owner").is_some()
        || matches!(command.intent(), peritus_app_protocol::WorkbenchIntent::ContinueExecution(_))
    {
        owner::require(&record.input, &original, owner)?;
    }
    if let Some(result) = record.result {
        let response = retained_response(&result, &original)?;
        if !matches!(&response, AppResponsePayload::WorkbenchReceipt(receipt)
            if receipt.operation() == command.operation() && receipt.query() == command.query())
        {
            return Err(problem("The cached receipt belongs to another workbench operation"));
        }
        return continuation::drive_admitted(
            app, owner, &record_owner, &original, &command, response,
        )
        .await;
    }
    let retained_owner = owner::require(&record.input, &original, owner)?;
    match super::raw_request_owned(app, &retained_owner, AppRequestPayload::QueryWorkbenchReceipt(command.clone())).await {
        Ok(AppResponsePayload::WorkbenchReceipt(receipt))
            if receipt.operation() == command.operation() && receipt.query() == command.query() =>
        {
            let payload = AppResponsePayload::WorkbenchReceipt(receipt);
            retain_reconciled_response_owned(&record_owner, &original, payload.clone())?;
            continuation::drive_admitted(
                app, &retained_owner, &record_owner, &original, &command, payload,
            )
            .await
        }
        Ok(AppResponsePayload::Error(error)) if error.code() == AppErrorCode::InvalidIdentifier => {
            let response =
                retransmit(app, &retained_owner, &record_owner, &original, payload).await?;
            continuation::drive_admitted(
                app, &retained_owner, &record_owner, &original, &command, response,
            )
            .await
        }
        Ok(AppResponsePayload::Error(error)) => {
            Err(problem(format!("Daemon rejected receipt reconciliation: {error}")))
        }
        Ok(_) => Err(problem("Unexpected workbench receipt response")),
        Err(error) => Err(uncertain(format!(
            "The original workbench operation could not be reconciled: {}",
            error.0
        ))),
    }
}

pub fn retained_workbench_command(app: &App, operation: &str) -> Result<Option<WorkbenchCommand>> {
    let Some(record) = app.operation(&format!("daemon:{operation}"))?
    else {
        return Ok(None);
    };
    let frame = STANDARD
        .decode(record.input["frame"].as_str().ok_or_else(|| problem("Missing daemon request"))?)
        .map_err(problem)?;
    let AppMessage::Request(request) = peritus_app_protocol::decode_app_message(
        &frame,
        peritus_app_protocol::AppProtocolLimits::PRODUCTION,
    )
    .map_err(problem)?
    else {
        return Err(problem("Invalid retained daemon request"));
    };
    match request.payload() {
        AppRequestPayload::WorkbenchCommand(command) => Ok(Some(command.clone())),
        _ => Err(problem("Retained workbench stage is not a command")),
    }
}

fn retained_response(result: &Value, original: &AppRequestEnvelope) -> Result<AppResponsePayload> {
    let payload = decoded_response(result, original)?;
    if let AppResponsePayload::Error(error) = &payload {
        return Err(problem(format!("Daemon rejected the request: {error}")));
    }
    Ok(payload)
}

fn decoded_response(result: &Value, original: &AppRequestEnvelope) -> Result<AppResponsePayload> {
    let frame = STANDARD
        .decode(result["frame"].as_str().ok_or_else(|| problem("Missing daemon receipt"))?)
        .map_err(problem)?;
    let AppMessage::Response(response) = peritus_app_protocol::decode_app_message(
        &frame,
        peritus_app_protocol::AppProtocolLimits::PRODUCTION,
    )
    .map_err(problem)?
    else {
        return Err(problem("Invalid retained daemon receipt"));
    };
    if response.context() != original.context()
        || response.request_id() != original.request_id()
        || response.correlation_id() != original.correlation_id()
    {
        return Err(problem("The retained response belongs to another native request"));
    }
    Ok(response.payload().clone())
}

async fn retain_reconciled_response(
    app: &App,
    key: &str,
    original: &AppRequestEnvelope,
    payload: AppResponsePayload,
) -> Result<AppResponsePayload> {
    let Some(owner) = app.try_own_operation(key).await? else {
        // Inspection remains available while the original transmitter owns its publication.
        return Ok(payload);
    };
    let original = original.clone();
    tokio::task::spawn_blocking(move || {
        if let Some(result) = owner.get()?.and_then(|record| record.result) {
            return decoded_response(&result, &original);
        }
        retain_reconciled_response_owned(&owner, &original, payload.clone())?;
        Ok(payload)
    }).await.map_err(problem)?
}

fn retain_reconciled_response_owned(
    record_owner: &OperationOwner,
    original: &AppRequestEnvelope,
    payload: AppResponsePayload,
) -> Result<()> {
    let response = peritus_app_protocol::AppResponseEnvelope::new(
        original.context(),
        original.request_id(),
        original.correlation_id(),
        payload,
    );
    let frame = STANDARD.encode(
        encode_app_message(
            &AppMessage::Response(response),
            peritus_app_protocol::AppProtocolLimits::PRODUCTION,
        )
        .map_err(problem)?,
    );
    record_owner.settle(json!({"frame":frame}))
}

async fn retransmit(
    app: &App,
    owner: &NativeOwner,
    record_owner: &OperationOwner,
    original: &AppRequestEnvelope,
    payload: AppRequestPayload,
) -> Result<AppResponsePayload> {
    let required = required_features(&payload);
    let mut client = super::connect_owned(app, owner, &required).await?;
    let identity = Client::new_request_identity().map_err(problem)?;
    let response = client.request(identity, payload).await.map_err(|error| {
        uncertain(format!("The exact workbench retry has an unknown outcome: {error}"))
    })?;
    let payload = response.payload().clone();
    if let AppResponsePayload::Error(error) = &payload
        && matches!(
            error.code(),
            AppErrorCode::Internal | AppErrorCode::Backpressure | AppErrorCode::NotReady
        )
    {
        return Err(uncertain(format!("Daemon reported an indeterminate outcome: {error}")));
    }
    retain_reconciled_response_owned(record_owner, original, payload.clone())?;
    if let AppResponsePayload::Error(error) = &payload {
        return Err(problem(format!("Daemon rejected the request: {error}")));
    }
    Ok(payload)
}

fn required_features(payload: &AppRequestPayload) -> Vec<WellKnownProtocolFeature> {
    let mut required = payload.required_workbench_feature().into_iter().collect::<Vec<_>>();
    if matches!(payload, AppRequestPayload::ControlProductRun(_)) {
        required.push(WellKnownProtocolFeature::ProductRunArtifacts);
    }
    required
}
