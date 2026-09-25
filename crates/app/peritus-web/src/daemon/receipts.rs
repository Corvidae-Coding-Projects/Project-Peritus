//! Retain exact native requests before transmission. Never infer acceptance from connectivity.
use super::{
    App, AppRequestPayload, AppResponsePayload, Client, Duration, Result, Value, endpoint, json,
    problem, response,
};
use crate::{error::uncertain, state::Operation};
use base64::{Engine, engine::general_purpose::STANDARD};
use peritus_app_protocol::{AppErrorCode, AppMessage, AppRequestEnvelope, encode_app_message};

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
    app.update(|state| {
        state.operations.insert(
            key.clone(),
            Operation {
                input: json!({"command":"daemon-request","parent":operation,"frame":frame}),
                result: None,
            },
        );
        Ok(())
    })?;
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

/// This protocol's native Interact/Control requests have no durable receipt-query API.
/// A retained exact response is proof; a later similar-looking conversation is not.
pub fn observed(app: &App, operation: &str) -> Result<Option<Value>> {
    use peritus_app_protocol::{AppProtocolLimits, decode_app_message};
    let Some(record) = app.snapshot()?.operations.get(&format!("daemon:{operation}")).cloned()
    else {
        return Ok(None);
    };
    let Some(value) = record.result else { return Ok(None) };
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
    Ok(Some(response(envelope.payload().clone())?))
}
