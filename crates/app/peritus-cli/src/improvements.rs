//! Scriptable paged candidate inspection and explicit evaluation over authenticated daemon IPC.

use crate::{client::Client, error::CliError, id::hex, operation::response_error, output::Output};
use peritus_app_protocol::{
    AppRequestPayload, AppResponsePayload, ImprovementCandidateSummary, ImprovementPage,
    ImprovementRequest, ImprovementTextQuery, ImprovementTextReference, WellKnownProtocolFeature,
};
use peritus_types::{RunId, SessionId, Sha256Digest, WorkspaceId};
use serde_json::json;
use std::{ffi::OsStr, time::Duration};

pub async fn execute(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Option<Duration>,
    request: ImprovementRequest,
    output: &Output,
) -> Result<(), CliError> {
    let mut client = Client::connect(
        endpoint,
        session,
        timeout,
        &[
            WellKnownProtocolFeature::HarnessImprovements,
            WellKnownProtocolFeature::HarnessImprovementPages,
        ],
    )
    .await?;
    let workspace = request.workspace();
    let request = match request {
        ImprovementRequest::List(workspace) => {
            ImprovementRequest::ListPage { workspace, after: None }
        }
        request => request,
    };
    let response = client
        .request(
            Client::new_request_identity()?,
            AppRequestPayload::Improvements(request),
        )
        .await?;
    let AppResponsePayload::ImprovementPage(mut page) = response.payload().clone() else {
        return response_error(response.payload(), "paged improvement inbox");
    };
    let mut candidates = 0_u64;
    loop {
        validate_page(&page, workspace)?;
        for candidate in page.candidates() {
            candidates = candidates.saturating_add(1);
            emit_candidate(output, page.revision(), *candidate)?;
            stream_text(
                &mut client,
                output,
                workspace,
                candidate.id(),
                None,
                candidate.proposal(),
                "improvement-proposal-text",
            )
            .await?;
            stream_evidence(
                &mut client,
                output,
                workspace,
                candidate.id(),
                page.revision(),
            )
            .await?;
        }
        let Some(after) = page.next() else { break };
        page = improvement_page(
            &mut client,
            ImprovementRequest::ListPage { workspace, after: Some(after) },
        )
        .await?;
    }
    let human = if candidates == 0 {
        "No improvement suggestions. Collection never starts evaluation.".to_owned()
    } else {
        format!("{candidates} improvement suggestions shown.")
    };
    output.event(
        json!({
            "ok": true,
            "kind": "improvements-complete",
            "result": {"workspace": hex(workspace.as_bytes()), "candidates": candidates.to_string()}
        }),
        &human,
    )
}

fn emit_candidate(
    output: &Output,
    revision: u64,
    item: ImprovementCandidateSummary,
) -> Result<(), CliError> {
    let status = if item.dismissed() {
        "dismissed"
    } else if item.evaluation().is_some() {
        "evaluation requested"
    } else {
        "untested suggestion"
    };
    output.event(
        json!({
            "ok": true,
            "kind": "improvement-candidate",
            "result": {
                "revision": revision.to_string(),
                "id": hex(item.id().as_bytes()),
                "status": status,
                "dismissed": item.dismissed(),
                "proposal": reference(item.proposal()),
                "evidenceCount": item.evidence_count().to_string(),
                "evaluation": item.evaluation().map(|evaluation| json!({
                    "conversation": hex(evaluation.conversation().as_bytes()),
                    "run": hex(evaluation.run().as_bytes()),
                    "target": hex(evaluation.target().as_bytes())
                }))
            }
        }),
        &format!(
            "{} [{}] · {} supporting runs",
            hex(item.id().as_bytes()),
            status,
            item.evidence_count()
        ),
    )
}

async fn stream_evidence(
    client: &mut Client,
    output: &Output,
    workspace: WorkspaceId,
    candidate: Sha256Digest,
    revision: u64,
) -> Result<(), CliError> {
    let mut after = None;
    loop {
        let response = client
            .request(
                Client::new_request_identity()?,
                AppRequestPayload::Improvements(ImprovementRequest::EvidencePage {
                    workspace,
                    candidate,
                    revision,
                    after,
                }),
            )
            .await?;
        let AppResponsePayload::ImprovementEvidencePage(page) = response.payload() else {
            return response_error(response.payload(), "improvement evidence page");
        };
        if page.workspace() != workspace
            || page.candidate() != candidate
            || page.revision() != revision
        {
            return Err(CliError::protocol(
                "read improvement evidence",
                "daemon returned another candidate's evidence page",
            ));
        }
        for evidence in page.evidence() {
            output.event(
                json!({
                    "ok": true,
                    "kind": "improvement-evidence",
                    "result": {
                        "candidate": hex(candidate.as_bytes()),
                        "run": hex(evidence.run().as_bytes()),
                        "summary": reference(evidence.summary())
                    }
                }),
                &format!("Evidence run {}", hex(evidence.run().as_bytes())),
            )?;
            stream_text(
                client,
                output,
                workspace,
                candidate,
                Some(evidence.run()),
                evidence.summary(),
                "improvement-evidence-text",
            )
            .await?;
        }
        let Some(next) = page.next() else { break };
        after = Some(next);
    }
    Ok(())
}

#[allow(
    clippy::too_many_arguments,
    reason = "the immutable source scope remains explicit at the streaming boundary"
)]
async fn stream_text(
    client: &mut Client,
    output: &Output,
    workspace: WorkspaceId,
    candidate: Sha256Digest,
    run: Option<RunId>,
    source: ImprovementTextReference,
    kind: &str,
) -> Result<(), CliError> {
    let mut offset = 0_u64;
    loop {
        let query = ImprovementTextQuery::new(workspace, candidate, run, source, offset)?;
        let response = client
            .request(
                Client::new_request_identity()?,
                AppRequestPayload::Improvements(ImprovementRequest::ReadText(query)),
            )
            .await?;
        let AppResponsePayload::ImprovementTextPage(page) = response.payload() else {
            return response_error(response.payload(), "improvement text page");
        };
        if page.query() != query {
            return Err(CliError::protocol(
                "read improvement text",
                "daemon returned a different immutable text source",
            ));
        }
        output.event(
            json!({
                "ok": true,
                "kind": kind,
                "result": {
                    "candidate": hex(candidate.as_bytes()),
                    "run": run.map(|value| hex(value.as_bytes())),
                    "offset": offset.to_string(),
                    "text": page.text(),
                    "next": page.next().map(|value| value.to_string())
                }
            }),
            page.text(),
        )?;
        let Some(next) = page.next() else { break };
        offset = next;
    }
    Ok(())
}

async fn improvement_page(
    client: &mut Client,
    request: ImprovementRequest,
) -> Result<ImprovementPage, CliError> {
    let response = client
        .request(
            Client::new_request_identity()?,
            AppRequestPayload::Improvements(request),
        )
        .await?;
    match response.payload() {
        AppResponsePayload::ImprovementPage(page) => Ok(page.clone()),
        payload => {
            response_error(payload, "improvement page")?;
            unreachable!("response_error always rejects an unexpected payload")
        }
    }
}

fn validate_page(page: &ImprovementPage, workspace: WorkspaceId) -> Result<(), CliError> {
    if page.workspace() != workspace {
        return Err(CliError::protocol(
            "read improvement inbox",
            "daemon returned another workspace's improvement page",
        ));
    }
    Ok(())
}

fn reference(value: ImprovementTextReference) -> serde_json::Value {
    json!({"digest": hex(value.digest().as_bytes()), "bytes": value.bytes().to_string()})
}
