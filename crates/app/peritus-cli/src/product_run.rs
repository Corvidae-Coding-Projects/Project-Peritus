//! Scriptable inspection and control of daemon-owned product runs.

use std::{ffi::OsStr, fmt::Write as _, path::Path, process::Command, time::Duration};

use peritus_app_protocol::{
    AppRequestPayload, AppResponsePayload, ProductDeliverable, ProductRunContinuation,
    ProductRunControl, ProductRunControlAction, ProductRunPhase, ProductRunQuery,
    ProductRunSettlementSnapshot, ProductRunSnapshot,
};
use peritus_product_runner::ProductRunner;
use peritus_run_settlement::{
    CandidateCheckpoint, CandidateStage, EvidenceStatus, QualificationEvidence, RunSettlement,
};
use peritus_types::{RunId, SessionId};

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
    timeout: Duration,
    arguments: ProductRunArgs,
    output: &Output,
) -> Result<(), CliError> {
    let mut client = Client::connect(endpoint, session, timeout, &[]).await?;
    match arguments {
        ProductRunArgs::List => list(&mut client, output).await,
        ProductRunArgs::Show { run_id } => show(&mut client, run_id, output).await,
        ProductRunArgs::Continue { run_id, message } => {
            continue_run(&mut client, run_id, message, output).await
        }
        ProductRunArgs::Execute { run_id } => execute_candidate(&mut client, run_id, output).await,
        ProductRunArgs::Control { run_id, action, confirmed_digest } => {
            control(&mut client, run_id, action, confirmed_digest, output).await
        }
    }
}

async fn list(client: &mut Client, output: &Output) -> Result<(), CliError> {
    let runs = query(client, ProductRunQuery::recent()).await?;
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

async fn continue_run(
    client: &mut Client,
    run_id: RunId,
    message: String,
    output: &Output,
) -> Result<(), CliError> {
    let continuation = ProductRunContinuation::new(run_id, message)
        .map_err(|error| CliError::usage(error.to_string()))?;
    let identity = Client::new_request_identity()?;
    let response =
        client.request(identity, AppRequestPayload::ContinueProductRun(continuation)).await?;
    let run = observed_response(response.payload())?;
    output.success("product-run-continued", observed_json(&run), &observed_human(&run))
}

async fn control(
    client: &mut Client,
    run_id: RunId,
    action: ProductRunControlAction,
    confirmed_digest: Option<[u8; 32]>,
    output: &Output,
) -> Result<(), CliError> {
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
    let digest = *checkpoint.identity().candidate_digest().as_bytes();
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
    if current != checkpoint.identity().candidate_digest() {
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
    let response = client.request(identity, AppRequestPayload::QueryProductRuns(query)).await?;
    match response.payload() {
        AppResponsePayload::ProductRuns(runs) => Ok(runs
            .iter()
            .cloned()
            .map(|snapshot| ObservedRun { snapshot, settlement: None })
            .collect()),
        AppResponsePayload::ProductRunSettlements(runs) => {
            Ok(runs.iter().map(observed_settlement).collect())
        }
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

fn observed_json(run: &ObservedRun) -> serde_json::Value {
    let snapshot = &run.snapshot;
    let deliverable = snapshot.deliverable();
    serde_json::json!({
        "run_id": hex(snapshot.run_id().as_bytes()),
        "workspace_id": hex(snapshot.workspace_id().as_bytes()),
        "state": state_name(snapshot),
        "phase": format!("{:?}", snapshot.phase()),
        "cycle": snapshot.cycle(),
        "task": snapshot.task(),
        "status": snapshot.status(),
        "summary": snapshot.summary(),
        "diff": snapshot.diff(),
        "checks": snapshot.gates(),
        "review": snapshot.review(),
        "settlement": run.settlement.as_ref().map(|value| serde_json::json!({
            "disposition": format!("{:?}", value.disposition()),
            "cause": format!("{:?}", value.cause()),
            "candidate_digest": value.checkpoint().map(|checkpoint| hex(checkpoint.identity().candidate_digest().as_bytes())),
            "evidence": value.checkpoint().map(evidence_json),
        })),
        "deliverable": deliverable.map(deliverable_json),
    })
}

fn deliverable_json(value: &ProductDeliverable) -> serde_json::Value {
    serde_json::json!({
        "workspace": value.workspace_path(),
        "changed_paths": value.changed_paths(),
        "successful_commands": value.successful_commands(),
        "run_instructions": value.run_instructions(),
        "qualification": qualification_name(value.qualification()),
        "accepted": value.accepted(),
        "commit_revision": value.commit_revision(),
        "export_path": value.export_path(),
        "discarded": value.discarded(),
    })
}

fn evidence_json(value: &CandidateCheckpoint) -> serde_json::Value {
    serde_json::json!({
        "checks": evidence_name(value.gates()),
        "requirements": evidence_name(value.obligations()),
        "review": evidence_name(value.review()),
    })
}

fn observed_human(run: &ObservedRun) -> String {
    let snapshot = &run.snapshot;
    let mut text = format!(
        "{}  {}\n{}\n{}",
        hex(snapshot.run_id().as_bytes()),
        state_name(snapshot),
        snapshot.task(),
        snapshot.status(),
    );
    if !snapshot.summary().is_empty() {
        text.push_str("\n\n");
        text.push_str(snapshot.summary());
    }
    if let Some(settlement) = &run.settlement {
        let _ = write!(
            text,
            "\n\nStopped because: {:?}\nDisposition: {:?}",
            settlement.cause(),
            settlement.disposition(),
        );
        if let Some(checkpoint) = settlement.checkpoint() {
            let _ = write!(
                text,
                "\nCandidate digest: {}\nChecks: {}; requirements: {}; review: {}",
                hex(checkpoint.identity().candidate_digest().as_bytes()),
                evidence_name(checkpoint.gates()),
                evidence_name(checkpoint.obligations()),
                evidence_name(checkpoint.review()),
            );
        }
    }
    if let Some(deliverable) = snapshot.deliverable() {
        let _ = write!(
            text,
            "\n\nWorkspace: {}\nQualification: {}\nChanged paths:\n{}\nSuccessful commands:\n{}\nRun: {}",
            deliverable.workspace_path(),
            qualification_name(deliverable.qualification()),
            display_list(deliverable.changed_paths()),
            display_list(deliverable.successful_commands()),
            deliverable.run_instructions(),
        );
    }
    text
}

fn display_list(values: &[String]) -> String {
    if values.is_empty() { "  (none)".to_owned() } else { format!("  {}", values.join("\n  ")) }
}

const fn state_name(snapshot: &ProductRunSnapshot) -> &'static str {
    match (snapshot.phase(), snapshot.deliverable()) {
        (ProductRunPhase::Complete, _) => "Accepted",
        (ProductRunPhase::WaitingForUser, _) => "Waiting for you",
        (ProductRunPhase::Failed, Some(_)) => "Candidate available",
        (ProductRunPhase::Failed, None) => "Stopped with no candidate",
        (ProductRunPhase::Cancelled, Some(_)) => "Cancelled; candidate available",
        (ProductRunPhase::Cancelled, None) => "Cancelled",
        (ProductRunPhase::RecoveryRequired, _) => "Recovery required",
        _ => "Running",
    }
}

const fn qualification_name(value: CandidateStage) -> &'static str {
    match value {
        CandidateStage::Observed => "observed",
        CandidateStage::Changed => "changed",
        CandidateStage::SelfChecked => "self-checked",
        CandidateStage::GatesPassed => "checks passed",
        CandidateStage::ReviewPending => "review pending",
        CandidateStage::Qualified => "qualified",
    }
}

fn missing_evidence(value: &CandidateCheckpoint) -> String {
    [("checks", value.gates()), ("requirements", value.obligations()), ("review", value.review())]
        .into_iter()
        .filter_map(|(name, evidence)| {
            let state = evidence_name(evidence);
            (state != "passed").then(|| format!("{name} {state}"))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

const fn evidence_name(value: &EvidenceStatus<QualificationEvidence>) -> &'static str {
    match value {
        EvidenceStatus::Missing => "missing",
        EvidenceStatus::Failed(_) => "failed",
        EvidenceStatus::Stale(_) => "stale",
        EvidenceStatus::Current(record) => {
            if record.value().satisfied() {
                "passed"
            } else {
                "failed"
            }
        }
    }
}

#[cfg(unix)]
fn run_command(root: &str, instruction: &str) -> Result<std::process::ExitStatus, CliError> {
    Command::new("sh").args(["-lc", instruction]).current_dir(Path::new(root)).status().map_err(
        |error| CliError::local_io("run candidate", Some(Path::new(root).to_owned()), error),
    )
}

#[cfg(windows)]
fn run_command(root: &str, instruction: &str) -> Result<std::process::ExitStatus, CliError> {
    Command::new("cmd").args(["/C", instruction]).current_dir(Path::new(root)).status().map_err(
        |error| CliError::local_io("run candidate", Some(Path::new(root).to_owned()), error),
    )
}
