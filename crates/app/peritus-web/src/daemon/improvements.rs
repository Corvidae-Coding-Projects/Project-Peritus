//! Candidate inbox bridge; mutations retain the original native request and response.

use super::{
    App, AppRequestPayload, NativeOwner, Result, Value, bytes, json, owner_for_project, problem,
};
use peritus_app_protocol::{
    ImprovementEvaluationRequest, ImprovementEvidencePage, ImprovementInbox, ImprovementPage,
    ImprovementPageCursor, ImprovementRequest, ImprovementText, ImprovementTextPage,
    ImprovementTextQuery, ImprovementTextReference, ProductProviderSelection,
};
use peritus_types::{ProviderProfileId, RunId, Sha256Digest, WorkspaceId};

async fn workspace(app: &App, project: &str) -> Result<(NativeOwner, WorkspaceId, Value)> {
    let project = app.project(project)?;
    let (owner, facts) = owner_for_project(app, &project).await?;
    let workspace = WorkspaceId::new(bytes(
        facts["workspace"]["id"]
            .as_str()
            .ok_or_else(|| problem("Register this project in workspace setup first"))?,
    )?)
    .map_err(|e| problem(format!("{e:?}")))?;
    Ok((owner, workspace, facts))
}

pub(super) fn projection(inbox: &ImprovementInbox) -> Value {
    json!({"workspace":crate::state::hex(inbox.workspace().as_bytes()), "candidates":inbox.candidates().iter().map(|item| json!({
        "id":crate::state::hex(item.id().as_bytes()), "proposal":item.proposal().as_str(), "dismissed":item.dismissed(),
        "evaluation":item.evaluation().map(|evaluation|json!({
            "conversation":crate::state::hex(evaluation.conversation().as_bytes()),
            "run":crate::state::hex(evaluation.run().as_bytes()),
            "target":crate::state::hex(evaluation.target().as_bytes())
        })),
        "evidence":item.evidence().iter().map(|e|json!({"run":crate::state::hex(e.run().as_bytes()),"digest":crate::state::hex(e.digest().as_bytes()),"summary":e.summary().as_str()})).collect::<Vec<_>>()
    })).collect::<Vec<_>>()})
}

pub(super) fn page_projection(page: &ImprovementPage) -> Value {
    json!({
        "workspace": crate::state::hex(page.workspace().as_bytes()),
        "revision": page.revision().to_string(),
        "candidates": page.candidates().iter().map(|item| json!({
            "id": crate::state::hex(item.id().as_bytes()),
            "proposal": reference(item.proposal()),
            "evidenceCount": item.evidence_count().to_string(),
            "dismissed": item.dismissed(),
            "evaluation": item.evaluation().map(|evaluation| json!({
                "conversation": crate::state::hex(evaluation.conversation().as_bytes()),
                "run": crate::state::hex(evaluation.run().as_bytes()),
                "target": crate::state::hex(evaluation.target().as_bytes())
            }))
        })).collect::<Vec<_>>(),
        "next": page.next().map(cursor)
    })
}

pub(super) fn evidence_projection(page: &ImprovementEvidencePage) -> Value {
    json!({
        "workspace": crate::state::hex(page.workspace().as_bytes()),
        "candidate": crate::state::hex(page.candidate().as_bytes()),
        "revision": page.revision().to_string(),
        "evidence": page.evidence().iter().map(|item| json!({
            "run": crate::state::hex(item.run().as_bytes()),
            "summary": reference(item.summary())
        })).collect::<Vec<_>>(),
        "next": page.next().map(cursor)
    })
}

pub(super) fn text_projection(page: &ImprovementTextPage) -> Value {
    json!({
        "offset": page.query().offset().to_string(),
        "text": page.text(),
        "next": page.next().map(|value| value.to_string())
    })
}

pub async fn list(app: &App, project: &str, after: &str) -> Result<Value> {
    let (owner, workspace, _) = workspace(app, project).await?;
    super::response(
        super::request_owned(
            app,
            &owner,
            AppRequestPayload::Improvements(ImprovementRequest::ListPage {
                workspace,
                after: page_cursor(after, workspace, None)?,
            }),
        )
        .await?,
    )
}

pub async fn evidence(
    app: &App,
    project: &str,
    candidate: &str,
    revision: &str,
    after: &str,
) -> Result<Value> {
    let (owner, workspace, _) = workspace(app, project).await?;
    let candidate = candidate_digest(candidate)?;
    super::response(
        super::request_owned(
            app,
            &owner,
            AppRequestPayload::Improvements(ImprovementRequest::EvidencePage {
                workspace,
                candidate,
                revision: revision.parse().map_err(problem)?,
                after: page_cursor(after, workspace, Some(candidate))?,
            }),
        )
        .await?,
    )
}

pub async fn text(
    app: &App,
    project: &str,
    candidate: &str,
    run: &str,
    source: &str,
    offset: u64,
) -> Result<Value> {
    let (owner, workspace, _) = workspace(app, project).await?;
    let candidate = candidate_digest(candidate)?;
    let run = (!run.is_empty())
        .then(|| RunId::new(bytes(run)?).map_err(|error| problem(format!("{error:?}"))))
        .transpose()?;
    let (digest, length) = source
        .split_once(':')
        .ok_or_else(|| problem("Expected a digest-bound improvement text source"))?;
    let source = ImprovementTextReference::new(
        candidate_digest(digest)?,
        length.parse().map_err(problem)?,
    )
    .map_err(problem)?;
    super::response(
        super::request_owned(
            app,
            &owner,
            AppRequestPayload::Improvements(ImprovementRequest::ReadText(
                ImprovementTextQuery::new(workspace, candidate, run, source, offset)
                    .map_err(problem)?,
            )),
        )
        .await?,
    )
}

pub async fn action(app: &App, input: &Value) -> Result<Value> {
    let string = |key: &str| input[key].as_str().unwrap_or("");
    let (owner, workspace_id, _) = workspace(app, string("project")).await?;
    let request = match string("action") {
        "suggest" => ImprovementRequest::Suggest {
            workspace: workspace_id,
            run: RunId::new(bytes(string("run"))?).map_err(|e| problem(format!("{e:?}")))?,
            proposal: ImprovementText::new(string("proposal").into()).map_err(problem)?,
        },
        "dismiss" => ImprovementRequest::Dismiss {
            workspace: workspace_id,
            candidate: candidate_digest(string("candidate"))?,
        },
        "evaluate" => {
            let (target_owner, target_workspace, target_facts) =
                workspace(app, string("target")).await?;
            if !owner.same_connection(&target_owner) {
                return Err(problem(
                    "The improvement source and evaluation target belong to different native owners",
                ));
            }
            let provider = target_facts["providers"][0]["id"]
                .as_str()
                .ok_or_else(|| problem("Configure a provider before evaluating suggestions"))?;
            let provider =
                ProviderProfileId::new(bytes(provider)?).map_err(|e| problem(format!("{e:?}")))?;
            let run =
                RunId::new(bytes(&crate::state::id()?)?).map_err(|e| problem(format!("{e:?}")))?;
            ImprovementRequest::Evaluate {
                workspace: workspace_id,
                candidate: candidate_digest(string("candidate"))?,
                evaluation: ImprovementEvaluationRequest::new(
                    run,
                    target_workspace,
                    ProductProviderSelection::new(provider, provider, provider),
                ),
            }
        }
        _ => return Err(problem("Unknown improvement action")),
    };
    let response = super::receipts::recorded(
        app,
        &owner,
        string("operation"),
        AppRequestPayload::Improvements(request),
    )
    .await?;
    super::response(response)
}

fn candidate_digest(value: &str) -> Result<Sha256Digest> {
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(problem("Expected a 64-digit candidate identity"));
    }
    let mut result = [0; 32];
    result[..16].copy_from_slice(&bytes(&value[..32])?);
    result[16..].copy_from_slice(&bytes(&value[32..])?);
    Ok(Sha256Digest::new(result))
}

fn reference(value: ImprovementTextReference) -> Value {
    json!({
        "digest": crate::state::hex(value.digest().as_bytes()),
        "bytes": value.bytes().to_string(),
        "source": format!(
            "{}:{}",
            crate::state::hex(value.digest().as_bytes()),
            value.bytes()
        )
    })
}

fn cursor(value: ImprovementPageCursor) -> String {
    format!(
        "{}:{}:{}",
        value.revision(),
        value.highwater_sequence(),
        value.sequence()
    )
}

fn page_cursor(
    value: &str,
    workspace: WorkspaceId,
    candidate: Option<Sha256Digest>,
) -> Result<Option<ImprovementPageCursor>> {
    if value.is_empty() {
        return Ok(None);
    }
    let mut parts = value.split(':');
    let revision = parts.next().ok_or_else(|| problem("Incomplete improvement cursor"))?;
    let highwater = parts.next().ok_or_else(|| problem("Incomplete improvement cursor"))?;
    let sequence = parts.next().ok_or_else(|| problem("Incomplete improvement cursor"))?;
    if parts.next().is_some() {
        return Err(problem("Malformed improvement cursor"));
    }
    Ok(Some(ImprovementPageCursor::snapshot(
        workspace,
        candidate,
        revision.parse().map_err(problem)?,
        highwater.parse().map_err(problem)?,
        sequence.parse().map_err(problem)?,
    )
    .map_err(problem)?))
}
