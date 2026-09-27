//! Retain exact native requests before transmission. Never infer acceptance from connectivity.
use super::{
    App, AppRequestPayload, AppResponsePayload, Client, Duration, Result, Value, endpoint, json,
    problem, response,
};
use crate::error::uncertain;
use base64::{Engine, engine::general_purpose::STANDARD};
use peritus_app_protocol::{
    AppErrorCode, AppMessage, AppRequestEnvelope, ProductActivityKind, ProductRunControlAction,
    ProductRunConversationQuery, ProductRunPhase, encode_app_message,
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
        Client::connect(endpoint.as_os_str(), None, Duration::from_secs(30), &required)
            .await
            .map_err(problem)?;
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
        AppRequestPayload::Interact(request) => request.request().run_id(),
        AppRequestPayload::ControlProductRun(control)
            if control.action() == ProductRunControlAction::Retry =>
        {
            control.run_id()
        }
        _ => return Ok(Value::Null),
    };
    match super::raw_request(
        app,
        AppRequestPayload::QueryInteraction(ProductRunConversationQuery::new(run_id)),
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
        AppRequestPayload::Interact(request) => Some(request.request().run_id()),
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
        AppRequestPayload::QueryInteraction(ProductRunConversationQuery::new(run_id)),
    )
    .await
    else {
        return Ok(None);
    };
    let AppResponsePayload::Interaction(snapshot) = current else { return Ok(None) };
    let proven = match request {
        AppRequestPayload::Interact(interaction) => {
            let previous = if baseline["missing"] == true {
                Some(0)
            } else {
                baseline["received"].as_str().and_then(|value| value.parse::<u64>().ok())
            };
            previous.is_some_and(|previous| snapshot.received() == previous.saturating_add(1))
                && snapshot
                    .activities()
                    .iter()
                    .rev()
                    .find(|activity| activity.kind() == ProductActivityKind::User)
                    .is_some_and(|activity| activity.text() == interaction.request().task())
        }
        AppRequestPayload::UpdateModels(update) => snapshot.models() == update.models(),
        AppRequestPayload::ControlProductRun(control) => control_postcondition(
            control.action(),
            snapshot.snapshot().phase(),
            snapshot.snapshot().deliverable(),
            baseline,
        ),
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
    phase: ProductRunPhase,
    deliverable: Option<&peritus_app_protocol::ProductDeliverable>,
    baseline: &Value,
) -> bool {
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
        ProductRunControlAction::Accept | ProductRunControlAction::Commit => false,
    }
}
