//! Native text messages use the same durable conversation ledger as the terminal client.
use super::{
    App, AppRequestPayload, AppResponsePayload, PreparedChat, Result, Value, hex, json,
    model_values, prepare, problem, readiness, receipts, response,
};
use peritus_app_protocol::{
    AppErrorCode, ControlOperationId, WorkbenchCommand, WorkbenchContinuation,
    WorkbenchExecutionSettings, WorkbenchExecutionState, WorkbenchInputId, WorkbenchInputOrder,
    WorkbenchInputText, WorkbenchIntent, WorkbenchNewInput, WorkbenchQueueIntent, WorkbenchReceipt,
};
use sha2::{Digest as _, Sha256};

pub async fn send(app: &App, input: &Value) -> Result<Value> {
    let operation = input["operation"]
        .as_str()
        .ok_or_else(|| problem("Missing original operation identity"))?;
    let prepared = prepare(app, input)?;
    app.retain_prepared_operation(operation, prepared.retained())?;
    readiness::ensure_ready(app, prepared.query.workspace(), prepared.providers).await?;
    drive(app, operation, prepared).await
}

pub async fn recover(app: &App, operation: &str) -> Result<Option<Value>> {
    let retained = app
        .snapshot()?
        .operations
        .get(operation)
        .and_then(|record| record.prepared.clone())
        .ok_or_else(|| {
            crate::error::uncertain(
                "The original message execution context is unavailable. Its outcome remains uncertain.",
            )
        })?;
    let prepared = PreparedChat::from_retained(&retained).map_err(|error| {
        crate::error::uncertain(format!(
            "The original message execution context is unreadable. Its outcome remains uncertain: {}",
            error.0
        ))
    })?;
    drive(app, operation, prepared).await.map(Some)
}

async fn drive(app: &App, operation: &str, prepared: PreparedChat) -> Result<Value> {
    let state = execution(app, prepared.query).await?;
    if let Some(state) = &state {
        validate_execution(state, &prepared)?;
    }
    let recover_create =
        receipts::retained_workbench_command(app, &stage(operation, "create"))?.is_some();
    let created = if state.is_none() || recover_create {
        let receipt = command(
            app,
            operation,
            "create",
            WorkbenchCommand::new(
                operation_id(operation, "create")?,
                prepared.query,
                0,
                WorkbenchIntent::CreateConversation(prepared.title.clone()),
            ),
        )
        .await?;
        Some(receipt)
    } else {
        None
    };
    if let (Some(state), Some(receipt)) = (&state, &created)
        && state.snapshot().revision() < receipt.accepted_revision()
    {
        return Err(problem("Conversation discovery is older than its accepted create receipt"));
    }
    let revision = state.as_ref().map_or_else(
        || created.as_ref().unwrap().accepted_revision(),
        |state| state.snapshot().revision(),
    );
    let retained = receipts::retained_workbench_command(app, &stage(operation, "queue"))?;
    let proposed = if let Some(retained) = retained {
        if retained.query() != prepared.query
            || retained.operation() != operation_id(operation, "queue")?
        {
            return Err(problem("Retained message admission identity changed"));
        }
        retained
    } else {
        let intent = if prepared.attachments.is_empty()
            && prepared.text.len() <= peritus_app_protocol::MAX_WORKBENCH_INPUT_BYTES
        {
            WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(
                WorkbenchNewInput::new(
                    input_id(operation)?,
                    WorkbenchInputText::new(prepared.text.clone()).map_err(problem)?,
                    WorkbenchInputOrder::new(Vec::new()).map_err(problem)?,
                )
                .map_err(problem)?,
            ))
        } else {
            super::chat_upload::prepare(app, &prepared, revision).await?
        };
        WorkbenchCommand::new(operation_id(operation, "queue")?, prepared.query, revision, intent)
    };
    let queued = command(app, operation, "queue", proposed).await?;
    after_queue(app, operation, prepared, queued).await
}

async fn after_queue(
    app: &App,
    operation: &str,
    prepared: PreparedChat,
    queued: WorkbenchReceipt,
) -> Result<Value> {
    let state = match execution(app, prepared.query).await {
        Ok(Some(state)) => state,
        Ok(None) => {
            return Ok(blocked_projection(
                &prepared,
                &queued,
                false,
                "The durable conversation could not be observed after its input was accepted.",
            ));
        }
        Err(error) if error.1 => return Err(error),
        Err(error) => return Ok(blocked_projection(&prepared, &queued, false, &error.0)),
    };
    if let Err(error) = validate_execution(&state, &prepared) {
        return Ok(blocked_projection(&prepared, &queued, false, &error.0));
    }
    let recover_start =
        receipts::retained_workbench_command(app, &stage(operation, "start"))?.is_some();
    let mut started = None;
    if state.run().is_none() || recover_start {
        match command(
            app,
            operation,
            "start",
            WorkbenchCommand::new(
                operation_id(operation, "start")?,
                prepared.query,
                queued.accepted_revision(),
                WorkbenchIntent::StartExecution(WorkbenchExecutionSettings::new(
                    prepared.run,
                    prepared.providers,
                    prepared.mode,
                    prepared.models.clone(),
                )),
            ),
        )
        .await
        {
            Ok(receipt) => started = Some(receipt),
            Err(error) if error.1 => return Err(error),
            Err(error) => return Ok(blocked_projection(&prepared, &queued, false, &error.0)),
        }
    }
    let Some(run) = state.run() else {
        return match interaction(app, prepared.run).await {
            Ok(observed) => response(AppResponsePayload::Interaction(observed)),
            Err(error) => Ok(blocked_projection(
                &prepared,
                started.as_ref().unwrap_or(&queued),
                true,
                &error.0,
            )),
        };
    };
    let observed = match interaction(app, run).await {
        Ok(observed) => observed,
        Err(error) => return Ok(blocked_projection(&prepared, &queued, false, &error.0)),
    };
    let current = observed.snapshot().operation();
    if state.has_goal() || !current.may_start_execution() {
        return response(AppResponsePayload::Interaction(observed));
    }
    match receipts::workbench_continuation(
        app,
        &stage(operation, "continue"),
        WorkbenchContinuation::bound(prepared.query, prepared.mode, queued.operation()),
        &observed,
    )
    .await
    {
        Ok(value) => response(value),
        Err(error) if error.1 => Err(error),
        Err(error) => Ok(blocked_projection(&prepared, &queued, false, &error.0)),
    }
}

fn blocked_projection(
    prepared: &PreparedChat,
    receipt: &WorkbenchReceipt,
    started: bool,
    detail: &str,
) -> Value {
    let observation = if started {
        "Execution was durably admitted, but its current run observation is unavailable."
    } else {
        "The message is durable and pending; execution has not been confirmed."
    };
    let action = if started {
        "Open Workbench to inspect the current execution and its durable queue."
    } else {
        "Open Workbench to inspect the durable queue and use /run when execution is idle."
    };
    json!({
        "models":model_values(&prepared.models),
        "mode":format!("{:?}",prepared.mode).to_lowercase(),
        "workbench":{
            "conversation":hex(prepared.query.conversation().as_bytes()),
            "revision":receipt.accepted_revision().to_string(),
            "queued":true,
            "started":started,
            "observation":observation,
            "action":action,
            "detail":detail
        }
    })
}

async fn execution(
    app: &App,
    query: peritus_app_protocol::WorkbenchQuery,
) -> Result<Option<WorkbenchExecutionState>> {
    match super::raw_request(app, AppRequestPayload::QueryWorkbenchExecution(query)).await? {
        AppResponsePayload::WorkbenchExecution(state) => Ok(Some(state)),
        AppResponsePayload::Error(error) if error.code() == AppErrorCode::InvalidIdentifier => {
            Ok(None)
        }
        AppResponsePayload::Error(error) => {
            Err(problem(format!("Daemon rejected conversation discovery: {error}")))
        }
        _ => Err(problem("Unexpected workbench execution response")),
    }
}

async fn interaction(
    app: &App,
    run: peritus_types::RunId,
) -> Result<peritus_app_protocol::ProductInteractionSnapshot> {
    match super::raw_request(
        app,
        AppRequestPayload::QueryInteraction(peritus_app_protocol::ProductInteractionQuery::new(
            run,
        )),
    )
    .await?
    {
        AppResponsePayload::Interaction(snapshot) => Ok(snapshot),
        AppResponsePayload::Error(error) => {
            Err(problem(format!("Daemon rejected conversation observation: {error}")))
        }
        _ => Err(problem("Unexpected conversation observation response")),
    }
}

fn validate_execution(state: &WorkbenchExecutionState, prepared: &PreparedChat) -> Result<()> {
    if state.snapshot().query() != prepared.query {
        return Err(problem("Daemon returned another durable conversation"));
    }
    if state.snapshot().archived() {
        return Err(problem("Unarchive this conversation before sending another message"));
    }
    if state.run().is_some_and(|run| run != prepared.run) {
        return Err(problem("This browser session is bound to another execution identity"));
    }
    Ok(())
}

async fn command(
    app: &App,
    operation: &str,
    name: &str,
    proposed: WorkbenchCommand,
) -> Result<WorkbenchReceipt> {
    let stage = stage(operation, name);
    let command = if let Some(retained) = receipts::retained_workbench_command(app, &stage)? {
        if retained.operation() != proposed.operation()
            || retained.query() != proposed.query()
            || retained.intent() != proposed.intent()
        {
            return Err(problem("Retained workbench stage does not match the original message"));
        }
        retained
    } else {
        proposed
    };
    match receipts::workbench_command(app, &stage, command.clone()).await? {
        AppResponsePayload::WorkbenchReceipt(receipt)
            if receipt.operation() == command.operation() && receipt.query() == command.query() =>
        {
            Ok(receipt)
        }
        _ => Err(problem("Unexpected workbench command response")),
    }
}

fn stage(operation: &str, name: &str) -> String {
    format!("{operation}:workbench:{name}")
}

fn operation_id(operation: &str, stage: &str) -> Result<ControlOperationId> {
    ControlOperationId::new(identity(operation, stage))
        .map_err(|error| problem(format!("{error:?}")))
}

fn input_id(operation: &str) -> Result<WorkbenchInputId> {
    WorkbenchInputId::new(identity(operation, "input"))
        .map_err(|error| problem(format!("{error:?}")))
}

fn identity(operation: &str, kind: &str) -> [u8; 16] {
    let mut hash = Sha256::new();
    hash.update(b"peritus/web/workbench-operation/v1\0");
    hash.update(operation.as_bytes());
    hash.update([0]);
    hash.update(kind.as_bytes());
    let digest = hash.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_identities_are_stable_and_separate() {
        assert_eq!(identity("operation", "queue"), identity("operation", "queue"));
        assert_ne!(identity("operation", "queue"), identity("operation", "start"));
        assert_ne!(identity("operation", "queue"), identity("other", "queue"));
    }
}
