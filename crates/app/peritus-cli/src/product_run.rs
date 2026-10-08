//! Scriptable inspection and control of daemon-owned product runs.

mod presentation;

use std::{ffi::OsStr, path::Path, process::Command, time::Duration};

use peritus_app_protocol::{
    AppRequestPayload, AppResponsePayload, ProductArtifactQuery, ProductArtifactReference,
    ProductDeliverable, ProductDeliverableIndexKind, ProductDeliverableIndexQuery,
    ProductDeliverableIndexReference, ProductRunControl, ProductRunControlAction,
    ProductRunOperation, ProductRunReferenceQuery, ProductRunReferenceSnapshot, ProductRunSnapshot,
    WellKnownProtocolFeature, product_deliverable_index_reference,
};
use peritus_product_runner::ProductRunner;
use peritus_run_settlement::{CandidateStage, RunSettlement};
use peritus_tools_shell::ExecInput;
use peritus_types::{RunId, SessionId};

use presentation::{missing_evidence, observed_human, observed_json};

use crate::{
    args::ProductRunArgs, client::Client, error::CliError, id::hex, operation::response_error,
    output::Output,
};

struct ObservedRun {
    snapshot: ProductRunSnapshot,
    settlement: Option<RunSettlement>,
}

pub async fn execute(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Option<Duration>,
    arguments: ProductRunArgs,
    output: &Output,
) -> Result<(), CliError> {
    let mut client = Client::connect(
        endpoint,
        session,
        timeout,
        &[WellKnownProtocolFeature::ProductRunArtifacts],
    )
    .await?;
    match arguments {
        ProductRunArgs::List => list(&mut client, output).await,
        ProductRunArgs::Show { run_id } => show(&mut client, run_id, output).await,
        ProductRunArgs::Execute { run_id } => execute_candidate(&mut client, run_id, output).await,
        ProductRunArgs::Control { run_id, action, confirmed_digest } => {
            control(&mut client, run_id, action, confirmed_digest, output).await
        }
    }
}

async fn list(client: &mut Client, output: &Output) -> Result<(), CliError> {
    let mut runs = Vec::new();
    let mut query = ProductRunReferenceQuery::first();
    loop {
        let identity = Client::new_request_identity()?;
        let response = client
            .request(identity, AppRequestPayload::QueryProductRunReferences(query))
            .await?;
        let AppResponsePayload::ProductRunReferencePage(page) = response.payload() else {
            return response_error(response.payload(), "list product runs")
                .and_then(|()| Err(CliError::protocol("list product runs", "missing run page")));
        };
        for entry in page.entries() {
            runs.push(hydrate_reference(client, entry.snapshot()).await?);
        }
        let Some(next) = page.next() else { break };
        query = ProductRunReferenceQuery::after(next);
    }
    let json = runs.iter().map(observed_json).collect::<Vec<_>>();
    let human = if runs.is_empty() {
        "No product runs.".to_owned()
    } else {
        runs.iter().map(observed_human).collect::<Vec<_>>().join("\n\n")
    };
    output.success("product-runs", json, &human)
}

async fn show(client: &mut Client, run_id: RunId, output: &Output) -> Result<(), CliError> {
    let run = query_exact(client, run_id).await?;
    output.success("product-run", observed_json(&run), &observed_human(&run))
}

async fn control(
    client: &mut Client,
    run_id: RunId,
    action: ProductRunControlAction,
    confirmed_digest: Option<[u8; 32]>,
    output: &Output,
) -> Result<(), CliError> {
    let observed = query_exact(client, run_id).await?;
    if !observed.snapshot.operation().legal_controls().allows(action) {
        let operation = observed.snapshot.operation();
        let reason = if operation.uncertainty().is_empty() {
            operation.known()
        } else {
            operation.uncertainty()
        };
        return Err(CliError::remote_failure(
            "control product run",
            format!(
                "the daemon does not admit {action:?} for operation {}: {reason}",
                operation.identity(),
            ),
        ));
    }
    if matches!(action, ProductRunControlAction::Accept | ProductRunControlAction::Commit) {
        confirm_unqualified(client, run_id, confirmed_digest).await?;
    }
    let identity = Client::new_request_identity()?;
    let response = client
        .request(
            identity,
            AppRequestPayload::ControlProductRun(ProductRunControl::new(run_id, action)),
        )
        .await?;
    if matches!(response.payload(), AppResponsePayload::Error(_)) {
        response_error(response.payload(), "control product run")?;
    }
    let run = query_exact(client, run_id).await?;
    output.success("product-run-controlled", observed_json(&run), &observed_human(&run))
}

async fn confirm_unqualified(
    client: &mut Client,
    run_id: RunId,
    confirmed_digest: Option<[u8; 32]>,
) -> Result<(), CliError> {
    let run = query_exact(client, run_id).await?;
    let deliverable = run
        .snapshot
        .deliverable()
        .ok_or_else(|| CliError::usage("the selected run has no candidate deliverable"))?;
    if deliverable.qualification() == CandidateStage::Qualified {
        return Ok(());
    }
    let checkpoint =
        run.settlement.as_ref().and_then(RunSettlement::checkpoint).ok_or_else(|| {
            CliError::protocol(
                "confirm unqualified candidate",
                "daemon omitted the exact candidate settlement",
            )
        })?;
    let digest = *checkpoint.identity().repository_digest().as_bytes();
    if confirmed_digest != Some(digest) {
        return Err(CliError::usage(format!(
            "candidate is unqualified ({missing}); repeat with --confirm-unqualified {digest}",
            missing = missing_evidence(checkpoint),
            digest = hex(&digest),
        )));
    }
    Ok(())
}

async fn execute_candidate(
    client: &mut Client,
    run_id: RunId,
    output: &Output,
) -> Result<(), CliError> {
    let run = query_exact(client, run_id).await?;
    let deliverable = run
        .snapshot
        .deliverable()
        .ok_or_else(|| CliError::usage("the selected run has no candidate to execute"))?;
    if deliverable.discarded() {
        return Err(CliError::usage("the selected candidate was discarded"));
    }
    let checkpoint =
        run.settlement.as_ref().and_then(RunSettlement::checkpoint).ok_or_else(|| {
            CliError::protocol("run exact candidate", "daemon omitted the candidate identity")
        })?;
    let workspace = Path::new(deliverable.workspace_path());
    let current = ProductRunner::candidate_digest(workspace).map_err(|error| {
        CliError::runtime("validate exact candidate", error.detail().to_owned())
    })?;
    if current != checkpoint.identity().repository_digest() {
        return Err(CliError::usage(
            "candidate changed after settlement; continue the run to inspect and requalify it",
        ));
    }
    let status = run_command(deliverable.workspace_path(), deliverable.run_instructions())?;
    if !status.success() {
        return Err(CliError::runtime(
            "run candidate",
            format!("candidate command exited with {status}"),
        ));
    }
    output.success(
        "product-run-executed",
        serde_json::json!({
            "run_id": hex(run_id.as_bytes()),
            "workspace": deliverable.workspace_path(),
            "command": deliverable.run_instructions(),
            "success": true,
        }),
        "candidate run command completed successfully",
    )
}

async fn query_exact(client: &mut Client, run_id: RunId) -> Result<ObservedRun, CliError> {
    let identity = Client::new_request_identity()?;
    let response = client
        .request(
            identity,
            AppRequestPayload::QueryProductRunReferences(ProductRunReferenceQuery::exact(run_id)),
        )
        .await?;
    let AppResponsePayload::ProductRunReferencePage(page) = response.payload() else {
        return response_error(response.payload(), "query exact product run")
            .and_then(|()| Err(CliError::protocol("query exact product run", "missing run page")));
    };
    if page.entries().len() != 1 || page.next().is_some() {
        return Err(CliError::protocol(
            "query exact product run",
            "daemon did not return exactly one product run",
        ));
    }
    hydrate_reference(client, page.entries()[0].snapshot()).await
}

async fn hydrate_reference(
    client: &mut Client,
    value: &ProductRunReferenceSnapshot,
) -> Result<ObservedRun, CliError> {
    let task = read_artifact(client, value.run_id(), value.task()).await?;
    let status = read_artifact(client, value.run_id(), value.status()).await?;
    let diff = read_optional_artifact(client, value.run_id(), value.diff()).await?;
    let gates = read_optional_artifact(client, value.run_id(), value.gates()).await?;
    let review = read_optional_artifact(client, value.run_id(), value.review()).await?;
    let summary = read_optional_artifact(client, value.run_id(), value.summary()).await?;
    let operation = value.operation();
    let operation = ProductRunOperation::new(
        operation.kind(),
        operation.state(),
        read_artifact(client, value.run_id(), operation.identity()).await?,
        read_artifact(client, value.run_id(), operation.known()).await?,
        read_optional_artifact(client, value.run_id(), operation.uncertainty()).await?,
        operation.legal_controls(),
    )
    .map_err(|error| CliError::protocol("hydrate product operation", error.to_string()))?;
    let mut snapshot = ProductRunSnapshot::new(
        value.run_id(),
        value.workspace_id(),
        value.providers(),
        value.phase(),
        value.cycle(),
        task,
        status,
        diff,
        gates,
        review,
        summary,
        operation,
    )
    .map_err(|error| CliError::protocol("hydrate product run", error.to_string()))?;
    if let Some(reference) = value.deliverable() {
        let workspace = read_artifact(client, value.run_id(), reference.workspace_path()).await?;
        let changed_paths = read_index(
            client,
            value.run_id(),
            ProductDeliverableIndexKind::ChangedPaths,
            reference.changed_paths(),
        )
        .await?;
        let successful_commands = read_index(
            client,
            value.run_id(),
            ProductDeliverableIndexKind::SuccessfulCommands,
            reference.successful_commands(),
        )
        .await?;
        let instructions =
            read_artifact(client, value.run_id(), reference.run_instructions()).await?;
        let mut deliverable = ProductDeliverable::candidate(
            workspace,
            changed_paths,
            successful_commands,
            instructions,
            reference.qualification(),
        )
        .map_err(|error| CliError::protocol("hydrate product deliverable", error.to_string()))?;
        if reference.accepted() {
            deliverable = deliverable.mark_accepted();
        }
        if let Some(commit) = reference.commit_revision() {
            deliverable = deliverable
                .mark_committed(read_artifact(client, value.run_id(), commit).await?)
                .map_err(|error| CliError::protocol("hydrate commit revision", error.to_string()))?;
        }
        if let Some(export) = reference.export_path() {
            deliverable = deliverable
                .mark_exported(read_artifact(client, value.run_id(), export).await?)
                .map_err(|error| CliError::protocol("hydrate export path", error.to_string()))?;
        }
        if reference.discarded() {
            deliverable = deliverable.mark_discarded();
        }
        snapshot = snapshot.with_deliverable(deliverable);
    }
    Ok(ObservedRun { snapshot, settlement: value.settlement() })
}

async fn read_optional_artifact(
    client: &mut Client,
    run: RunId,
    reference: Option<ProductArtifactReference>,
) -> Result<String, CliError> {
    match reference {
        Some(reference) => read_artifact(client, run, reference).await,
        None => Ok(String::new()),
    }
}

async fn read_artifact(
    client: &mut Client,
    run: RunId,
    reference: ProductArtifactReference,
) -> Result<String, CliError> {
    let mut bytes = Vec::new();
    let mut offset = 0;
    loop {
        let query = ProductArtifactQuery::new(run, reference, offset)
            .map_err(|error| CliError::protocol("read product artifact", error.to_string()))?;
        let identity = Client::new_request_identity()?;
        let response = client
            .request(identity, AppRequestPayload::QueryProductArtifact(query))
            .await?;
        let AppResponsePayload::ProductArtifactPage(page) = response.payload() else {
            return response_error(response.payload(), "read product artifact")
                .and_then(|()| Err(CliError::protocol("read product artifact", "missing page")));
        };
        if page.query() != query {
            return Err(CliError::protocol("read product artifact", "response changed its source"));
        }
        bytes.extend_from_slice(page.bytes());
        let Some(next) = page.next() else { break };
        offset = next;
    }
    if !reference.matches_bytes(&bytes) {
        return Err(CliError::protocol("read product artifact", "content digest or length changed"));
    }
    String::from_utf8(bytes)
        .map_err(|_| CliError::protocol("read product artifact", "content is not UTF-8"))
}

async fn read_index(
    client: &mut Client,
    run: RunId,
    kind: ProductDeliverableIndexKind,
    index: ProductDeliverableIndexReference,
) -> Result<Vec<String>, CliError> {
    let mut values = Vec::new();
    let mut after = None;
    loop {
        let query = ProductDeliverableIndexQuery::new(run, kind, index, after)
            .map_err(|error| CliError::protocol("read deliverable index", error.to_string()))?;
        let identity = Client::new_request_identity()?;
        let response = client
            .request(identity, AppRequestPayload::QueryProductDeliverableIndex(query))
            .await?;
        let AppResponsePayload::ProductDeliverableIndexPage(page) = response.payload() else {
            return response_error(response.payload(), "read deliverable index")
                .and_then(|()| Err(CliError::protocol("read deliverable index", "missing page")));
        };
        if page.query() != query {
            return Err(CliError::protocol("read deliverable index", "response changed its index"));
        }
        for entry in page.entries() {
            values.push(read_artifact(client, run, entry.value()).await?);
        }
        let Some(next) = page.next() else { break };
        after = Some(next);
    }
    if product_deliverable_index_reference(kind, &values)
        .map_err(|error| CliError::protocol("read deliverable index", error.to_string()))?
        != index
    {
        return Err(CliError::protocol("read deliverable index", "index root or count changed"));
    }
    Ok(values)
}

fn run_command(root: &str, instruction: &str) -> Result<std::process::ExitStatus, CliError> {
    let input = ExecInput::from_command_line(instruction).map_err(|error| {
        CliError::usage(format!("candidate run instruction is not executable: {}", error.detail()))
    })?;
    Command::new(input.executable())
        .args(input.arguments())
        .current_dir(Path::new(root))
        .status()
        .map_err(|error| {
            CliError::local_io("run candidate", Some(Path::new(root).to_owned()), error)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_command_executes_directly_and_rejects_shell_text() {
        assert!(
            run_command(env!("CARGO_MANIFEST_DIR"), "rustc --version")
                .expect("direct command")
                .success()
        );
        assert!(
            run_command(env!("CARGO_MANIFEST_DIR"), "rustc --version && rustc --version",).is_err()
        );
    }
}
