//! Candidate inbox bridge; mutations retain the original native request and response.

use super::{
    App, AppRequestPayload, AppResponsePayload, Result, Value, bytes, facts, json, problem,
};
use peritus_app_protocol::{
    ImprovementInbox, ImprovementRequest, ImprovementText, ProductProviderSelection,
    ProductRunRequest,
};
use peritus_types::{ProviderProfileId, RunId, Sha256Digest, WorkspaceId};

fn workspace(app: &App, project: &str) -> Result<WorkspaceId> {
    let facts = facts(app, &app.project(project)?)?;
    WorkspaceId::new(bytes(
        facts["workspace"]["id"]
            .as_str()
            .ok_or_else(|| problem("Register this project in workspace setup first"))?,
    )?)
    .map_err(|e| problem(format!("{e:?}")))
}

pub(super) fn projection(inbox: &ImprovementInbox) -> Value {
    json!({"workspace":crate::state::hex(inbox.workspace().as_bytes()), "candidates":inbox.candidates().iter().map(|item| json!({
        "id":crate::state::hex(item.id().as_bytes()), "proposal":item.proposal().as_str(), "dismissed":item.dismissed(),
        "evaluation":item.evaluation().map(|r|crate::state::hex(r.as_bytes())),
        "evidence":item.evidence().iter().map(|e|json!({"run":crate::state::hex(e.run().as_bytes()),"digest":crate::state::hex(e.digest().as_bytes()),"summary":e.summary().as_str()})).collect::<Vec<_>>()
    })).collect::<Vec<_>>()})
}

pub async fn list(app: &App, project: &str) -> Result<Value> {
    super::response(
        super::request(
            app,
            AppRequestPayload::Improvements(ImprovementRequest::List(workspace(app, project)?)),
        )
        .await?,
    )
}

pub async fn action(app: &App, input: &Value) -> Result<Value> {
    let string = |key: &str| input[key].as_str().unwrap_or("");
    let workspace_id = workspace(app, string("project"))?;
    let request = match string("action") {
        "suggest" => ImprovementRequest::Suggest {
            workspace: workspace_id,
            run: RunId::new(bytes(string("run"))?).map_err(|e| problem(format!("{e:?}")))?,
            proposal: ImprovementText::new(string("proposal").into()).map_err(problem)?,
        },
        "dismiss" => ImprovementRequest::Dismiss {
            workspace: workspace_id,
            candidate: candidate(string("candidate"))?,
        },
        "evaluate" => {
            let target = app.project(string("target"))?;
            let target_facts = facts(app, &target)?;
            let provider = target_facts["providers"][0]["id"]
                .as_str()
                .ok_or_else(|| problem("Configure a provider before evaluating suggestions"))?;
            let provider =
                ProviderProfileId::new(bytes(provider)?).map_err(|e| problem(format!("{e:?}")))?;
            let run =
                RunId::new(bytes(&crate::state::id()?)?).map_err(|e| problem(format!("{e:?}")))?;
            ImprovementRequest::Evaluate {
                workspace: workspace_id,
                candidate: candidate(string("candidate"))?,
                run: ProductRunRequest::new(
                    run,
                    workspace(app, &target.id)?,
                    ProductProviderSelection::new(provider, provider, provider),
                    "Evaluate selected harness suggestion".into(),
                )
                .map_err(problem)?,
            }
        }
        _ => return Err(problem("Unknown improvement action")),
    };
    let response = super::receipts::recorded(
        app,
        string("operation"),
        AppRequestPayload::Improvements(request),
    )
    .await?;
    if !matches!(response, AppResponsePayload::Improvements(_)) {
        return Err(problem("Unexpected improvement response"));
    }
    super::response(response)
}

fn candidate(value: &str) -> Result<Sha256Digest> {
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(problem("Expected a 64-digit candidate identity"));
    }
    let mut result = [0; 32];
    result[..16].copy_from_slice(&bytes(&value[..32])?);
    result[16..].copy_from_slice(&bytes(&value[32..])?);
    Ok(Sha256Digest::new(result))
}
