//! Scriptable inspection and control of daemon-owned product runs.

mod presentation;

use std::{ffi::OsStr, path::Path, process::Command, time::Duration};

use peritus_app_protocol::{
    AppRequestPayload, AppResponsePayload, ProductRunControl, ProductRunControlAction,
    ProductRunQuery, ProductRunSettlementSnapshot, ProductRunSnapshot,
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
    let mut client = Client::connect(endpoint, session, timeout, &[]).await?;
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
    loop {
        let offset = u64::try_from(runs.len())
            .map_err(|_| CliError::protocol("list product runs", "run offset overflowed"))?;
        let page = query(client, ProductRunQuery::page(offset)).await?;
        let complete = page.len() < peritus_app_protocol::MAX_PRODUCT_RUN_PAGE;
        runs.extend(page);
        if complete {
            break;
        }
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
    let run = observed_response(response.payload())?;
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
    let mut runs = query(client, ProductRunQuery::exact(run_id)).await?;
    if runs.len() != 1 {
        return Err(CliError::protocol(
            "query exact product run",
            "daemon did not return exactly one product run",
        ));
    }
    Ok(runs.remove(0))
}

async fn query(client: &mut Client, query: ProductRunQuery) -> Result<Vec<ObservedRun>, CliError> {
    let identity = Client::new_request_identity()?;
    let response =
        client.request(identity, AppRequestPayload::QueryProductRunObservations(query)).await?;
    match response.payload() {
        AppResponsePayload::ProductRunObservations(runs) => Ok(runs
            .iter()
            .map(|run| ObservedRun {
                snapshot: run.snapshot().clone(),
                settlement: run.settlement(),
            })
            .collect()),
        AppResponsePayload::ProductRunAccepted(snapshot) => {
            Ok(vec![ObservedRun { snapshot: snapshot.clone(), settlement: None }])
        }
        AppResponsePayload::ProductRunSettled(settled) => Ok(vec![observed_settlement(settled)]),
        payload => response_error(payload, "product-run query").map(|()| Vec::new()),
    }
}

fn observed_response(payload: &AppResponsePayload) -> Result<ObservedRun, CliError> {
    match payload {
        AppResponsePayload::ProductRunAccepted(snapshot) => {
            Ok(ObservedRun { snapshot: snapshot.clone(), settlement: None })
        }
        AppResponsePayload::ProductRunSettled(settled) => Ok(observed_settlement(settled)),
        payload => response_error(payload, "product-run response")
            .and_then(|()| Err(CliError::protocol("product run", "missing product run"))),
    }
}

fn observed_settlement(value: &ProductRunSettlementSnapshot) -> ObservedRun {
    ObservedRun { snapshot: value.snapshot().clone(), settlement: Some(*value.settlement()) }
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
