//! Native text messages use the same durable conversation ledger as the terminal client.
use super::{
    App, AppRequestPayload, AppResponsePayload, NativeOwner, PreparedChat, Result, Value, hex, json,
    model_values, prepare, problem, readiness, receipts, response,
};
use peritus_app_protocol::{
    AppErrorCode, ControlOperationId, WorkbenchCommand,
    WorkbenchExecutionSettings, WorkbenchExecutionState, WorkbenchInputId,
    WorkbenchIntent, WorkbenchQueueIntent, WorkbenchReceipt,
};
use sha2::{Digest as _, Sha256};
mod recovery;
mod sources;
pub use recovery::inspect;

fn message(input: &Value) -> Result<&str> {
    let text = input["text"].as_str().unwrap_or("");
    if text.trim().is_empty() || text.chars().any(|character| {
        character.is_control() && character != '\n' && character != '\t'
    }) {
        return Err(problem("The message is empty or contains terminal control characters"));
    }
    Ok(text)
}

pub async fn send(app: &App, input: &Value) -> Result<Value> {
    let operation = input["operation"].as_str()
        .ok_or_else(|| problem("Missing original operation identity"))?;
    let owner = app.own_operation(operation).await?;
    owner.insert(input.clone())?;
    if let Some(result) = owner.get()?.and_then(|record| record.result) { return Ok(result); }
    send_owned(app, input, &owner).await
}

pub(crate) async fn send_owned(app: &App, input: &Value, owner: &crate::state::OperationOwner) -> Result<Value> {
    let operation = input["operation"]
        .as_str()
        .ok_or_else(|| problem("Missing original operation identity"))?;
    if owner.identity() != operation {
        return Err(problem("Message preparation belongs to another operation owner"));
    }
    message(input)?;
    let prepared = prepare(app, input).await?;
    owner.retain_prepared(prepared.retained()?)?;
    readiness::ensure_ready(app, prepared.owner()?, prepared.providers).await?;
    drive(app, operation, prepared).await.map_err(|error| crate::error::uncertain(format!(
        "The exact message stages remain available for reconciliation or retry: {}", error.0,
    )))
}

pub async fn retry(app: &App, operation: &str) -> Result<Value> {
    let retained = app
        .operation(operation)?
        .and_then(|record| record.prepared)
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
    drive(app, operation, prepared).await.map_err(|error| crate::error::uncertain(format!(
        "The original message stages remain available for reconciliation or retry: {}", error.0,
    )))
}

async fn drive(app: &App, operation: &str, prepared: PreparedChat) -> Result<Value> {
    let owner = prepared.owner()?.clone();
    let state = execution(app, &owner, prepared.query).await?;
    if let Some(state) = &state {
        validate_execution(state, &prepared)?;
    }
    let recover_create =
        receipts::retained_workbench_command(app, &stage(operation, "create"))?.is_some();
    let created = if state.is_none() || recover_create {
        let receipt = command(
            app,
            &owner,
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
    let revision = sources::attach(app, operation, &prepared, revision).await?;
    let intent = sources::queue_intent(operation, &prepared)?;
    if let WorkbenchQueueIntent::EnqueueSource { source, .. } = &intent
        && receipts::retained_workbench_command(app, &stage(operation, "queue"))?.is_none()
    {
        sources::upload_message(app, &prepared, revision, *source).await?;
    }
    let queued = command(
        app,
        &owner,
        operation,
        "queue",
        WorkbenchCommand::new(
            operation_id(operation, "queue")?,
            prepared.query,
            revision,
            WorkbenchIntent::Queue(intent),
        ),
    )
    .await?;
    after_queue(app, operation, prepared, queued).await
}

async fn after_queue(
    app: &App,
    operation: &str,
    prepared: PreparedChat,
    queued: WorkbenchReceipt,
) -> Result<Value> {
    let owner = prepared.owner()?.clone();
    let state = match execution(app, &owner, prepared.query).await {
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
            &owner,
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
        return match interaction(app, &owner, prepared.run).await {
            Ok(observed) => response(AppResponsePayload::Interaction(observed)),
            Err(error) => Ok(blocked_projection(
                &prepared,
                started.as_ref().unwrap_or(&queued),
                true,
                &error.0,
            )),
        };
    };
    let observed = match interaction(app, &owner, run).await {
        Ok(observed) => observed,
        Err(error) => return Ok(blocked_projection(&prepared, &queued, false, &error.0)),
    };
    let recover_continue =
        receipts::retained_workbench_command(app, &stage(operation, "continue"))?.is_some();
    let current = observed.snapshot().operation();
    if !recover_continue && (state.has_goal() || !current.may_start_execution()) {
        return response(AppResponsePayload::Interaction(observed));
    }
    match command(
        app, &owner, operation, "continue",
        WorkbenchCommand::new(
            operation_id(operation, "continue")?, prepared.query, state.snapshot().revision(),
            WorkbenchIntent::ContinueExecution(WorkbenchExecutionSettings::new(
                prepared.run, prepared.providers, prepared.mode, prepared.models.clone(),
            )),
        ),
    ).await {
        Ok(receipt) => match interaction(app, &owner, run).await {
            Ok(observed) => response(AppResponsePayload::Interaction(observed)),
            Err(error) => Ok(blocked_projection(&prepared, &receipt, true, &error.0)),
        },
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
    owner: &NativeOwner,
    query: peritus_app_protocol::WorkbenchQuery,
) -> Result<Option<WorkbenchExecutionState>> {
    if hex(query.workspace().as_bytes()) != owner.workspace() {
        return Err(problem("The conversation belongs to another native workspace owner"));
    }
    match super::raw_request_owned(app, owner, AppRequestPayload::QueryWorkbenchExecution(query)).await? {
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
    owner: &NativeOwner,
    run: peritus_types::RunId,
) -> Result<peritus_app_protocol::ProductInteractionSnapshot> {
    match super::raw_request_owned(
        app,
        owner,
        AppRequestPayload::QueryInteraction(peritus_app_protocol::ProductInteractionQuery::new(
            run,
        )),
    )
    .await?
    {
        AppResponsePayload::Interaction(snapshot)
            if snapshot.snapshot().run_id() == run
                && hex(snapshot.snapshot().workspace_id().as_bytes()) == owner.workspace() => Ok(snapshot),
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
    owner: &NativeOwner,
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
    match receipts::workbench_command(app, owner, &stage, command.clone()).await? {
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
