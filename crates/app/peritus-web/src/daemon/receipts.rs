//! Retain exact native requests before transmission. Never infer acceptance from connectivity.
use super::{
    App, AppRequestPayload, AppResponsePayload, Client, Result, Value, endpoint, json, problem,
    response,
};
use crate::error::uncertain;
use base64::{Engine, engine::general_purpose::STANDARD};
use peritus_app_protocol::{
    AppErrorCode, AppMessage, AppRequestEnvelope, ProductInteractionQuery, ProductRunControlAction,
    ProductRunPhase, WorkbenchCommand, encode_app_message,
};

pub async fn recorded(
    app: &App,
    operation: &str,
    payload: AppRequestPayload,
) -> Result<AppResponsePayload> {
    let key = format!("daemon:{operation}");
    if app.snapshot()?.operations.contains_key(&key) {
        return Err(uncertain(
            "The original native request was already submitted. Inspect the conversation and its operation record; it will not be sent again.",
        ));
    }
    let baseline = recovery_baseline(app, &payload).await?;
    let endpoint = endpoint(app)?;
    let required = payload.required_workbench_feature().into_iter().collect::<Vec<_>>();
    let mut client =
        Client::connect(endpoint.as_os_str(), None, None, &required).await.map_err(problem)?;
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
    app.record_operation(
        key.clone(),
        json!({"command":"daemon-request","parent":operation,"frame":frame,"baseline":baseline}),
    )?;
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
    app.update(|state| {
        state.operations.get_mut(&key).ok_or_else(|| problem("Daemon receipt missing"))?.result =
            Some(result);
        Ok(())
    })
    .map_err(|e| uncertain(e.0))?;
    if let AppResponsePayload::Error(error) = response.payload() {
        return Err(problem(format!("Daemon rejected the request: {error}")));
    }
    Ok(response.payload().clone())
}

/// Sends one domain-idempotent workbench command or reconciles its original durable operation.
/// A missing authoritative receipt permits retransmission only with the same command identity.
pub async fn workbench_command(
    app: &App,
    operation: &str,
    command: WorkbenchCommand,
) -> Result<AppResponsePayload> {
    let key = format!("daemon:{operation}");
    let payload = AppRequestPayload::WorkbenchCommand(command.clone());
    let Some(record) = app.snapshot()?.operations.get(&key).cloned() else {
        return recorded(app, operation, payload).await;
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
    if let Some(result) = record.result {
        return retained_response(&result);
    }
    match super::raw_request(app, AppRequestPayload::QueryWorkbenchReceipt(command.clone())).await {
        Ok(AppResponsePayload::WorkbenchReceipt(receipt))
            if receipt.operation() == command.operation() && receipt.query() == command.query() =>
        {
            let payload = AppResponsePayload::WorkbenchReceipt(receipt);
            retain_reconciled_response(app, &key, &original, payload.clone())?;
            Ok(payload)
        }
        Ok(AppResponsePayload::Error(error)) if error.code() == AppErrorCode::InvalidIdentifier => {
            retransmit(app, &key, &original, payload).await
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
    let Some(record) = app.snapshot()?.operations.get(&format!("daemon:{operation}")).cloned()
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

/// Reconciles the workbench continuation request against a fresh authoritative interaction.
/// Retransmission is legal only while the exact durable input remains pending on a terminal run.
pub async fn workbench_continuation(
    app: &App,
    operation: &str,
    continuation: peritus_app_protocol::WorkbenchContinuation,
    observed: &peritus_app_protocol::ProductInteractionSnapshot,
) -> Result<AppResponsePayload> {
    let key = format!("daemon:{operation}");
    let payload = AppRequestPayload::ContinueWorkbenchExecution(continuation);
    let Some(record) = app.snapshot()?.operations.get(&key).cloned() else {
        return recorded(app, operation, payload).await;
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
        return Err(problem("The continuation operation identity was reused for different input"));
    }
    if let Some(result) = record.result {
        return retained_response(&result);
    }
    let operation = observed.snapshot().operation();
    let resume_admissible = operation.may_start_execution();
    if observed.incorporated() >= observed.received() || !resume_admissible {
        let recovered = AppResponsePayload::Interaction(observed.clone());
        retain_reconciled_response(app, &key, &original, recovered.clone())?;
        return Ok(recovered);
    }
    retransmit(app, &key, &original, payload).await
}

fn retained_response(result: &Value) -> Result<AppResponsePayload> {
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
    if let AppResponsePayload::Error(error) = response.payload() {
        return Err(problem(format!("Daemon rejected the request: {error}")));
    }
    Ok(response.payload().clone())
}

fn retain_reconciled_response(
    app: &App,
    key: &str,
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
    app.update(|state| {
        state.operations.get_mut(key).ok_or_else(|| problem("Daemon receipt missing"))?.result =
            Some(json!({"frame":frame}));
        Ok(())
    })
}

async fn retransmit(
    app: &App,
    key: &str,
    original: &AppRequestEnvelope,
    payload: AppRequestPayload,
) -> Result<AppResponsePayload> {
    let endpoint = endpoint(app)?;
    let required = payload.required_workbench_feature().into_iter().collect::<Vec<_>>();
    let mut client =
        Client::connect(endpoint.as_os_str(), None, None, &required).await.map_err(problem)?;
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
    retain_reconciled_response(app, key, original, payload.clone())?;
    if let AppResponsePayload::Error(error) = &payload {
        return Err(problem(format!("Daemon rejected the request: {error}")));
    }
    Ok(payload)
}

/// Returns a retained exact response, or proves the requested postcondition from a fresh durable
/// run observation and the pre-request baseline. It never retransmits an uncertain mutation.
pub async fn observed(app: &App, operation: &str) -> Result<Option<Value>> {
    use peritus_app_protocol::{AppProtocolLimits, decode_app_message};
    let Some(record) = app.snapshot()?.operations.get(&format!("daemon:{operation}")).cloned()
    else {
        return Ok(None);
    };
    let frame = STANDARD
        .decode(record.input["frame"].as_str().ok_or_else(|| problem("Missing daemon request"))?)
        .map_err(problem)?;
    let AppMessage::Request(request) =
        decode_app_message(&frame, AppProtocolLimits::PRODUCTION).map_err(problem)?
    else {
        return Err(problem("Invalid retained daemon request"));
    };
    if let Some(value) = record.result {
        let frame = STANDARD
            .decode(value["frame"].as_str().ok_or_else(|| problem("Missing daemon receipt"))?)
            .map_err(problem)?;
        let AppMessage::Response(envelope) =
            decode_app_message(&frame, AppProtocolLimits::PRODUCTION).map_err(problem)?
        else {
            return Err(problem("Invalid retained daemon receipt"));
        };
        if let AppResponsePayload::Error(error) = envelope.payload() {
            return Ok(Some(json!({"error":format!("Daemon rejected the request: {error}")})));
        }
        return Ok(Some(response(envelope.payload().clone())?));
    }
    reconcile(app, request.payload(), &record.input["baseline"]).await
}

async fn recovery_baseline(app: &App, payload: &AppRequestPayload) -> Result<Value> {
    let run_id = match payload {
        AppRequestPayload::ControlProductRun(control) => control.run_id(),
        _ => return Ok(Value::Null),
    };
    match super::raw_request(
        app,
        AppRequestPayload::QueryInteraction(ProductInteractionQuery::new(run_id)),
    )
    .await?
    {
        AppResponsePayload::Interaction(snapshot) => {
            response(AppResponsePayload::Interaction(snapshot))
        }
        AppResponsePayload::Error(error) if error.code() == AppErrorCode::InvalidIdentifier => {
            Ok(json!({"missing":true}))
        }
        _ => Ok(Value::Null),
    }
}

const fn run_id(payload: &AppRequestPayload) -> Option<peritus_types::RunId> {
    match payload {
        AppRequestPayload::UpdateModels(update) => Some(update.run_id()),
        AppRequestPayload::ControlProductRun(control) => Some(control.run_id()),
        _ => None,
    }
}

async fn reconcile(
    app: &App,
    request: &AppRequestPayload,
    baseline: &Value,
) -> Result<Option<Value>> {
    let Some(run_id) = run_id(request) else { return Ok(None) };
    let Ok(current) = super::raw_request(
        app,
        AppRequestPayload::QueryInteraction(ProductInteractionQuery::new(run_id)),
    )
    .await
    else {
        return Ok(None);
    };
    let AppResponsePayload::Interaction(snapshot) = current else { return Ok(None) };
    let proven = match request {
        AppRequestPayload::UpdateModels(update) => snapshot.models() == update.models(),
        AppRequestPayload::ControlProductRun(control) => {
            control_postcondition(control.action(), snapshot.snapshot(), baseline)
        }
        _ => false,
    };
    if !proven {
        return Ok(None);
    }
    let mut recovered = response(AppResponsePayload::Interaction(snapshot))?;
    if let Some(object) = recovered.as_object_mut() {
        object.insert("recovered".to_owned(), Value::Bool(true));
    }
    Ok(Some(recovered))
}

fn control_postcondition(
    action: ProductRunControlAction,
    snapshot: &peritus_app_protocol::ProductRunSnapshot,
    baseline: &Value,
) -> bool {
    let phase = snapshot.phase();
    let deliverable = snapshot.deliverable();
    match action {
        ProductRunControlAction::Cancel => phase == ProductRunPhase::Cancelled,
        ProductRunControlAction::Retry => {
            matches!(
                baseline["run"]["phase"].as_str(),
                Some("Failed" | "Cancelled" | "RecoveryRequired")
            ) && baseline["run"]["phase"] != format!("{phase:?}")
        }
        ProductRunControlAction::Export => {
            deliverable.is_some_and(|value| !value.export_path().is_empty())
        }
        ProductRunControlAction::Discard => {
            deliverable.is_some_and(peritus_app_protocol::ProductDeliverable::discarded)
        }
        ProductRunControlAction::Acknowledge => {
            baseline["run"]["operation"]["legalControls"]["acknowledge"] == true
                && baseline["run"]["operation"]["identity"] != snapshot.operation().identity()
        }
        ProductRunControlAction::Accept | ProductRunControlAction::Commit => false,
    }
}
