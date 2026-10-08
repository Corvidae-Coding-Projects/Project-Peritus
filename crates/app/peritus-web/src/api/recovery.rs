//! Inspection, retry and review have separate effect ownership.
use super::{App, Json, Result, State, Value, body, daemon, dispatch, json, mutation, operations, problem};
use std::sync::Arc;

pub(super) async fn review(
    State(app): State<Arc<App>>, body::JsonInput(input): body::JsonInput,
) -> Result<Json<Value>> {
    mutation::workspace(&app, &input)?;
    if input["confirmed"] != true {
        return Err(problem("Confirm that you inspected the original target"));
    }
    let id = input["operation"]
        .as_str()
        .ok_or_else(|| problem("Choose the original operation"))?;
    operations::validate_recovery_identity(id)?;
    let lock = app.lock(format!("operation:{id}"))?;
    let _guard = lock.lock().await;
    Ok(Json(operations::acknowledge(&app, id).await?))
}

pub(super) async fn retry(
    State(app): State<Arc<App>>, body::JsonInput(input): body::JsonInput,
) -> Result<Json<Value>> {
    mutation::workspace(&app, &input)?;
    if input["confirmed"] != true {
        return Err(problem("An explicit retry of the original operation is required"));
    }
    let id = operation(&input)?;
    let lock = app.lock(format!("operation:{id}"))?;
    let guard = lock.lock_owned().await;
    let observed = operations::observe(&app, id).await?;
    let owner = app.own_operation(id).await?;
    let record = owner.get()?
        .ok_or_else(|| problem("The original operation is not in this workspace ledger"))?;
    if record.result.as_ref().is_some_and(|result| result["retryable"] != true) {
        return Ok(Json(observed));
    }
    let command = record.input["command"].as_str().unwrap_or("");
    if !["send", "config", "preferences", "file-save"].contains(&command) {
        return Err(problem("This native effect has no durable retry identity. Inspect its outcome before issuing a new action."));
    }
    let resource_guard = mutation::guard(&app, &record.input).await?;
    if command == "file-save" {
        let ownership = super::super::files::edit::RetryOwner { receipt: owner, operation: guard, resource: resource_guard };
        let result = super::super::files::edit::retry(Arc::clone(&app), ownership, record.input, input["payload"].clone()).await;
        return match result {
            Ok(_) => Ok(Json(operations::observe(&app, id).await?)),
            Err(error) => Ok(Json(json!({"error":error.0,"uncertain":error.1,"operation":id}))),
        };
    }
    let _guard = guard;
    let _resource_guard = resource_guard;
    let result = if command == "send" && record.prepared.is_some() {
        daemon::retry_send(&app, id).await
    } else {
        dispatch(&app, &record.input, &owner).await
    };
    let result = match result {
        Ok(result) => result,
        Err(error) if error.1 => return Ok(Json(json!({"error":error.0,"uncertain":true,"operation":id}))),
        Err(error) => json!({"error":error.0}),
    };
    owner.complete_retry(result).map_err(|error| crate::error::uncertain(error.0))?;
    Ok(Json(operations::observe(&app, id).await?))
}

pub(super) async fn cancel(
    State(app): State<Arc<App>>, body::JsonInput(input): body::JsonInput,
) -> Result<Json<Value>> {
    mutation::workspace(&app, &input)?;
    let id = operation(&input)?;
    let record = app
        .operation(id)?
        .ok_or_else(|| problem("The original operation is not in this workspace ledger"))?;
    if record.input["command"] != "git" {
        return Err(problem("Only owned Git effects support this cancellation route"));
    }
    Ok(Json(super::super::git::cancel(&app, id, record.prepared.as_ref()).await?))
}

fn operation(input: &Value) -> Result<&str> {
    let id = input["operation"].as_str().ok_or_else(|| problem("Choose the original operation"))?;
    operations::validate_identity(id)?;
    Ok(id)
}
