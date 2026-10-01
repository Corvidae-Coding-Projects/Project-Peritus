//! Honest observation of a run-backed or prepared durable conversation.

use super::{
    App, AppErrorCode, AppRequestPayload, AppResponsePayload, ConversationId,
    ProductInteractionQuery, Result, RunId, Value, WorkbenchQuery, WorkspaceId, bytes, facts, hex,
    json, problem, raw_request, response,
};

pub(super) async fn observe(app: &App, session_id: &str) -> Result<Value> {
    let session = app.session(session_id)?;
    let run = RunId::new(bytes(&session.run)?).map_err(|e| problem(format!("{e:?}")))?;
    match raw_request(app, AppRequestPayload::QueryInteraction(ProductInteractionQuery::new(run)))
        .await?
    {
        AppResponsePayload::Interaction(value) => response(AppResponsePayload::Interaction(value)),
        AppResponsePayload::Error(error) if error.code() == AppErrorCode::InvalidIdentifier => {
            prepared(app, &session, error.actionable_message()).await
        }
        AppResponsePayload::Error(error) => Err(problem(format!(
            "Daemon rejected conversation observation: {}",
            error.actionable_message()
        ))),
        _ => Err(problem("Unexpected conversation response from daemon")),
    }
}

async fn prepared(app: &App, session: &crate::state::Session, run_error: String) -> Result<Value> {
    let project = app.project(&session.project)?;
    let facts = facts(app, &project)?;
    let workspace = WorkspaceId::new(bytes(
        facts["workspace"]["id"]
            .as_str()
            .ok_or_else(|| problem("The session project has no workspace identity"))?,
    )?)
    .map_err(|e| problem(format!("{e:?}")))?;
    let conversation = ConversationId::new(bytes(&session.conversation)?)
        .map_err(|e| problem(format!("{e:?}")))?;
    let query = WorkbenchQuery::new(conversation, workspace);
    match raw_request(app, AppRequestPayload::QueryWorkbenchExecution(query)).await? {
        AppResponsePayload::WorkbenchExecution(state) if state.snapshot().query() == query => {
            let started = state.run().is_some();
            Ok(json!({"workbench":{
                "conversation":hex(conversation.as_bytes()),
                "revision":state.snapshot().revision().to_string(),
                "queued":!started,
                "started":started,
                "observation":if started {"Execution was admitted, but its run is not currently observable."} else {"The evaluation inputs are durable and execution has not been admitted."},
                "action":"Open Workbench to inspect the durable queue and resume the exact evaluation request.",
                "detail":run_error
            }}))
        }
        AppResponsePayload::Error(workbench_error) => Err(problem(format!(
            "Run and durable workbench observation failed: {run_error}; {}",
            workbench_error.actionable_message()
        ))),
        _ => Err(problem("Unexpected durable workbench response")),
    }
}
