//! Real product-run composition for one `HarnessBench` adapter invocation.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::Instant,
};

use peritus_product_runner::{
    ProductDeliveryScope, ProductRunInput, ProductRunOutcome, ProductRunUpdate, ProductRunner,
    RunObserver,
};
use peritus_provider_core::CancellationToken;
use peritus_types::RunId;
use sha2::{Digest, Sha256};

use crate::{
    BenchmarkError,
    args::HarnessBenchInput,
    evidence::{ProductObservation, RelocatablePaths, RunReport},
    providers, session, trace, workspace,
};

pub async fn run_harnessbench(input: HarnessBenchInput) -> Result<RunReport, BenchmarkError> {
    let started = Instant::now();
    let baseline = workspace::prepare(&input.workspace)?;
    let sandbox = input.sandbox.canonicalize().map_err(|error| {
        BenchmarkError::filesystem("canonicalize sandbox", &input.sandbox, error)
    })?;
    let prompt = fs::read_to_string(&input.prompt_file).map_err(|error| {
        BenchmarkError::filesystem("read benchmark prompt", &input.prompt_file, error)
    })?;
    let evidence_dir = sandbox.join("peritus-benchmark");
    fs::create_dir_all(&evidence_dir).map_err(|error| {
        BenchmarkError::filesystem("create benchmark evidence directory", &evidence_dir, error)
    })?;
    let conversation = session::BenchmarkSession::open(
        &evidence_dir,
        &input.session_id,
        &input.task_id,
        &input.prompt_file,
        prompt.clone(),
    )?;
    let trace_path = conversation.current_trace_path();
    let usage_proxy = sandbox.join("usage-proxy");
    let cancellation = CancellationToken::new();
    let role_providers = providers::authenticated(&cancellation).await?;
    let (observer, last_observation) = observation_capture();
    let result = ProductRunner::run(
        ProductRunInput {
            run_id: run_id(&input.session_id, &input.task_id)?,
            workspace_root: baseline.root.clone(),
            trace_path: trace_path.clone(),
            finding_state: String::new(),
            task: prompt.clone(),
            delivery_scope: ProductDeliveryScope::WorkspaceChanges,
            conversation: Arc::new(conversation.clone()),
            providers: role_providers,
            cancelled: Arc::new(AtomicBool::new(false)),
            provider_cancellation: cancellation,
        },
        observer,
    )
    .await;
    let trace_inputs = conversation.trace_inputs();
    let projected_responses = trace::publish_harnessbench(
        &trace_inputs,
        &usage_proxy,
        &input.task_id,
        &input.session_id,
        &input.model_id,
    )?;
    let last_observation_path = publish_last_observation(&last_observation, &evidence_dir)?;
    let (session_trace_paths, relocatable_paths) = retained_paths(
        &sandbox,
        &baseline.root,
        &trace_path,
        &trace_inputs,
        &usage_proxy,
        last_observation_path.as_deref(),
    )?;
    let (success, summary, changed_paths, failure_kind, failure) = outcome_fields(result);
    let report = RunReport {
        schema_version: 4,
        success,
        task_id: input.task_id,
        session_id: input.session_id,
        harness_model_id: input.model_id,
        workspace: baseline.root,
        baseline_head: baseline.head,
        initialized_repository: baseline.initialized_repository,
        created_artifact_manifest: baseline.created_artifact_manifest,
        writer: format!("openai/{}", providers::WRITER_MODEL),
        reviewer: format!("anthropic/{}", providers::REVIEWER_MODEL),
        elapsed_ms: started.elapsed().as_millis(),
        trace_path,
        conversation_turn: conversation.turn_number(),
        session_trace_paths,
        usage_proxy,
        projected_responses,
        last_observation_path,
        relocatable_paths,
        summary,
        changed_paths,
        failure_kind,
        failure,
    };
    report.publish(&evidence_dir)?;
    Ok(report)
}

type OutcomeFields = (bool, Option<String>, Vec<PathBuf>, Option<String>, Option<String>);

fn outcome_fields(
    result: Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>,
) -> OutcomeFields {
    match result {
        Ok(ProductRunOutcome::Complete(output)) => {
            (true, Some(output.summary), output.changed_paths, None, None)
        }
        Ok(ProductRunOutcome::WaitingForUser { question, .. }) => {
            (false, None, Vec::new(), Some("waiting_for_user".to_owned()), Some(question))
        }
        Err(error) => (
            false,
            None,
            Vec::new(),
            Some(format!("{:?}", error.kind()).to_lowercase()),
            Some(error.to_string()),
        ),
    }
}

fn retained_paths(
    sandbox: &Path,
    workspace: &Path,
    trace_path: &Path,
    trace_inputs: &[(PathBuf, String)],
    usage_proxy: &Path,
    last_observation_path: Option<&Path>,
) -> Result<(Vec<PathBuf>, RelocatablePaths), BenchmarkError> {
    let session = trace_inputs.iter().map(|(path, _)| path.clone()).collect::<Vec<_>>();
    let relocatable = RelocatablePaths::new(
        sandbox,
        workspace,
        trace_path,
        &session,
        usage_proxy,
        last_observation_path,
    )?;
    Ok((session, relocatable))
}

pub type ObservationCapture = Arc<Mutex<Option<ProductRunUpdate>>>;

pub fn observation_capture() -> (RunObserver, ObservationCapture) {
    let capture = Arc::new(Mutex::new(None));
    let sink = Arc::clone(&capture);
    let observer: RunObserver = Arc::new(move |update| {
        eprintln!(
            "[peritus-benchmark] {:?} cycle {}: {}",
            update.phase, update.cycle, update.status
        );
        if let Ok(mut last) = sink.lock() {
            *last = Some(update);
        }
    });
    (observer, capture)
}

pub fn publish_last_observation(
    capture: &ObservationCapture,
    directory: &Path,
) -> Result<Option<PathBuf>, BenchmarkError> {
    capture
        .lock()
        .ok()
        .and_then(|mut observation| observation.take())
        .map(ProductObservation::from_update)
        .map(|observation| observation.publish(directory))
        .transpose()
}

pub fn run_id(session_id: &str, task_id: &str) -> Result<RunId, BenchmarkError> {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus.external-benchmark.run.v1\0");
    hasher.update(session_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(task_id.as_bytes());
    let digest = hasher.finalize();
    let mut identity = [0_u8; 16];
    identity.copy_from_slice(&digest[..16]);
    RunId::new(identity)
        .map_err(|_| BenchmarkError::Workspace("derived run identity is zero".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_identity_is_stable_and_task_scoped() {
        assert_eq!(run_id("session", "task").unwrap(), run_id("session", "task").unwrap());
        assert_ne!(run_id("session", "task").unwrap(), run_id("session", "other").unwrap());
    }
}
