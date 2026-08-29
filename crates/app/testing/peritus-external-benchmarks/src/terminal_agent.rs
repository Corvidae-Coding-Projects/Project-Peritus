//! Real product-run composition for one Terminal-Bench task environment.

use std::{
    fs,
    sync::{Arc, atomic::AtomicBool},
    time::Instant,
};

use peritus_product_runner::{
    ConversationView, ProductDeliveryScope, ProductRunInput, ProductRunOutcome, ProductRunner,
};
use peritus_provider_core::CancellationToken;

use crate::{
    BenchmarkError,
    agent::{observation_capture, publish_last_observation, run_id},
    args::TerminalBenchInput,
    evidence::TerminalBenchReport,
    identity::BenchmarkAgentIdentity,
    providers, session, trace, workspace,
};

pub async fn run(input: TerminalBenchInput) -> Result<TerminalBenchReport, BenchmarkError> {
    let started = Instant::now();
    let agent_identity = BenchmarkAgentIdentity::current()?;
    let baseline = workspace::prepare(&input.workspace)?;
    fs::create_dir_all(&input.evidence_dir).map_err(|error| {
        BenchmarkError::filesystem(
            "create Terminal-Bench evidence directory",
            &input.evidence_dir,
            error,
        )
    })?;
    let evidence_dir = input.evidence_dir.canonicalize().map_err(|error| {
        BenchmarkError::filesystem(
            "canonicalize Terminal-Bench evidence directory",
            &input.evidence_dir,
            error,
        )
    })?;
    let prompt = fs::read_to_string(&input.prompt_file).map_err(|error| {
        BenchmarkError::filesystem("read benchmark prompt", &input.prompt_file, error)
    })?;
    let task = prompt.clone();
    let conversation = session::BenchmarkSession::open(
        &evidence_dir,
        &input.session_id,
        &input.task_id,
        &input.prompt_file,
        prompt,
    )?;
    let trace_path = conversation.current_trace_path();
    let cancellation = CancellationToken::new();
    let role_providers = providers::authenticated(&cancellation).await?;
    let (observer, last_observation) = observation_capture();
    let result = ProductRunner::run(
        ProductRunInput {
            run_id: run_id(&input.session_id, &input.task_id)?,
            workspace_root: baseline.root.clone(),
            trace_path: trace_path.clone(),
            finding_state: String::new(),
            task,
            delivery_scope: ProductDeliveryScope::AuthorizedExternalEffects,
            conversation: Arc::new(conversation.clone()),
            providers: role_providers,
            cancelled: Arc::new(AtomicBool::new(false)),
            provider_cancellation: cancellation,
        },
        observer,
    )
    .await;
    let usage = trace::summarize_usage(&trace_path, &conversation.render())?;
    let last_observation_path = publish_last_observation(&last_observation, &evidence_dir)?;
    let (success, summary, changed_paths, failure_kind, failure) = match result {
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
    };
    let report = TerminalBenchReport {
        schema_version: 2,
        agent_identity,
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
        usage,
        last_observation_path,
        summary,
        changed_paths,
        failure_kind,
        failure,
    };
    report.publish(&evidence_dir)?;
    Ok(report)
}
