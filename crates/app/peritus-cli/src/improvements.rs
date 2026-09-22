//! Scriptable candidate collection and explicit evaluation over authenticated daemon IPC.

use crate::{client::Client, error::CliError, id::hex, operation::response_error, output::Output};
use peritus_app_protocol::{
    AppRequestPayload, AppResponsePayload, ImprovementRequest, WellKnownProtocolFeature,
};
use peritus_types::SessionId;
use serde_json::json;
use std::{ffi::OsStr, time::Duration};

pub async fn execute(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Duration,
    request: ImprovementRequest,
    output: &Output,
) -> Result<(), CliError> {
    let mut client = Client::connect(
        endpoint,
        session,
        timeout,
        &[WellKnownProtocolFeature::HarnessImprovements],
    )
    .await?;
    let response = client
        .request(Client::new_request_identity()?, AppRequestPayload::Improvements(request))
        .await?;
    let AppResponsePayload::Improvements(inbox) = response.payload() else {
        return response_error(response.payload(), "improvement inbox");
    };
    let rows = inbox.candidates().iter().map(|item| json!({
        "id":hex(item.id().as_bytes()),"proposal":item.proposal().as_str(),"dismissed":item.dismissed(),
        "evaluation":item.evaluation().map(|r|hex(r.as_bytes())),
        "evidence":item.evidence().iter().map(|e|json!({"run":hex(e.run().as_bytes()),"digest":hex(e.digest().as_bytes()),"summary":e.summary().as_str()})).collect::<Vec<_>>()
    })).collect::<Vec<_>>();
    let human = if rows.is_empty() {
        "No improvement suggestions. Collection never starts evaluation.".into()
    } else {
        inbox
            .candidates()
            .iter()
            .map(|item| {
                format!(
                    "{} [{}]\n{}\nEvidence runs: {}{}",
                    hex(item.id().as_bytes()),
                    if item.dismissed() {
                        "dismissed"
                    } else if item.evaluation().is_some() {
                        "evaluation requested"
                    } else {
                        "untested suggestion"
                    },
                    item.proposal().as_str(),
                    item.evidence()
                        .iter()
                        .map(|e| hex(e.run().as_bytes()))
                        .collect::<Vec<_>>()
                        .join(", "),
                    item.evaluation().map_or_else(String::new, |r| format!(
                        "\nReview with: peritus runs show --run {}",
                        hex(r.as_bytes())
                    ))
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    output.success(
        "improvements",
        json!({"workspace":hex(inbox.workspace().as_bytes()),"candidates":rows}),
        &human,
    )
}
