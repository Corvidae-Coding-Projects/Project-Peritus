//! Observation of a message never transmits a workbench mutation.
use super::{
    App, AppResponsePayload, PreparedChat, Result, Value, blocked_projection, execution,
    interaction, operation_id, receipts, response, stage, validate_execution,
};
use crate::error::{problem, uncertain};
use peritus_app_protocol::{WorkbenchCommand, WorkbenchExecutionSettings, WorkbenchIntent};
use serde_json::json;

pub async fn inspect(app: &App, operation: &str) -> Result<Option<Value>> {
    let Some(retained) = app.operation(operation)?.and_then(|record| record.prepared)
        else { return Ok(None) };
    let prepared = PreparedChat::from_retained(&retained).map_err(|error| {
        uncertain(format!("The retained message context cannot be inspected: {}", error.0))
    })?;
    if let Some(command) = retained_stage(app, operation, "create", &prepared,
        &WorkbenchIntent::CreateConversation(prepared.title.clone()))?
        && let Some(AppResponsePayload::Error(error)) =
            receipts::inspect_workbench_command(app, &stage(operation, "create"), &command).await?
    {
        return Ok(Some(json!({"error":format!("Daemon rejected conversation creation: {error}")})));
    }
    let Some(command) = retained_stage(app, operation, "queue", &prepared,
        &WorkbenchIntent::Queue(super::sources::queue_intent(operation, &prepared)?))? else { return Ok(None) };
    let queued = match receipts::inspect_workbench_command(app, &stage(operation, "queue"), &command).await? {
        Some(AppResponsePayload::WorkbenchReceipt(receipt)) => receipt,
        Some(AppResponsePayload::Error(error)) => return Ok(Some(json!({
            "error":format!("Daemon rejected the original message queue operation: {error}"),
        }))),
        _ => return Ok(None),
    };
    let start_admitted = if let Some(command) = retained_stage(app, operation, "start", &prepared,
        &WorkbenchIntent::StartExecution(WorkbenchExecutionSettings::new(
            prepared.run, prepared.providers, prepared.mode, prepared.models.clone(),
        )))?
    {
        match receipts::inspect_workbench_command(app, &stage(operation, "start"), &command).await? {
            Some(AppResponsePayload::WorkbenchReceipt(_)) => true,
            Some(AppResponsePayload::Error(error)) => return Ok(Some(blocked_projection(
                &prepared, &queued, false,
                &format!("The original execution admission was rejected: {error}"),
            ))),
            _ => false,
        }
    } else { false };
    let Some(owner) = prepared.owner.as_ref() else {
        return receipts::observed(app, &stage(operation, "continue")).await;
    };
    let Some(state) = execution(app, owner, prepared.query).await? else { return Ok(None) };
    validate_execution(&state, &prepared)?;
    if let Some(run) = state.run() {
        if let Some(command) = retained_stage(app, operation, "continue", &prepared,
            &WorkbenchIntent::ContinueExecution(WorkbenchExecutionSettings::new(
                prepared.run, prepared.providers, prepared.mode, prepared.models.clone(),
            )))?
        {
            match receipts::inspect_workbench_command(app, &stage(operation, "continue"), &command).await? {
                Some(AppResponsePayload::Error(error)) => return Ok(Some(blocked_projection(
                    &prepared, &queued, false, &format!("The original continuation was rejected: {error}"),
                ))),
                Some(AppResponsePayload::WorkbenchReceipt(_)) => {
                    if receipts::inspect_continuation_admission(app, &stage(operation, "continue"), &command).await?
                        != Some(peritus_app_protocol::WorkbenchContinuationAdmissionState::LaunchOwned)
                    {
                        return Ok(None);
                    }
                }
                _ => return Ok(None),
            }
        } else if !start_admitted {
            // An existing run binding does not prove that this message reached its execution
            // stage. Explicit retry can finish the remaining exact stages without queueing twice.
            return Ok(None);
        }
        return response(AppResponsePayload::Interaction(interaction(app, owner, run).await?)).map(Some);
    }
    // Queue acceptance proves only the queued input, not execution admission. Keep the original
    // operation recoverable until explicit retry drives its remaining durable stages.
    Ok(None)
}

fn retained_stage(
    app: &App, operation: &str, name: &str, prepared: &PreparedChat, intent: &WorkbenchIntent,
) -> Result<Option<WorkbenchCommand>> {
    let command = receipts::retained_workbench_command(app, &stage(operation, name))?;
    if let Some(command) = &command
        && (command.operation() != operation_id(operation, name)?
            || command.query() != prepared.query || command.intent() != intent)
    {
        return Err(problem("The retained stage does not match the original message context"));
    }
    Ok(command)
}
