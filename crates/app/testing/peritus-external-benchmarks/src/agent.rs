//! Shared real-product composition for admitted external benchmark invocations.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, atomic::AtomicBool},
};

use peritus_product_runner::{
    ConversationView, ProductRunInput, ProductRunOutcome, ProductRunUpdate, ProductRunner,
    ProductRunnerError, ProductRunnerErrorKind, RunObserver,
};
use peritus_provider_core::CancellationToken;
use peritus_run_settlement::SettlementCause;
use peritus_types::RunId;
use sha2::{Digest, Sha256};

use crate::{
    BenchmarkError,
    admission::{self, AdmittedInvocation},
    args::HarnessBenchInput,
    candidate,
    evidence::{
        BenchmarkSuite, ProductObservation, QualificationReport, RelocatablePaths, ResourceReport,
    },
    providers, trace, workspace,
};

pub async fn run_harnessbench(
    input: HarnessBenchInput,
) -> Result<crate::RunReport, BenchmarkError> {
    execute(admission::harnessbench(input)?).await
}

pub async fn execute(
    admitted: AdmittedInvocation,
) -> Result<crate::evidence::InvocationReport, BenchmarkError> {
    let AdmittedInvocation {
        mut guard,
        prompt,
        conversation,
        evidence_dir,
        sandbox,
        max_elapsed,
        delivery_scope,
    } = admitted;
    if guard.seed().agent_identity.source_revision.is_none() {
        let error = BenchmarkError::Identity(
            "compiled source revision is absent; rebuild the benchmark agent from a clean revision"
                .to_owned(),
        );
        return guard.fail(SettlementCause::Adapter, &error);
    }
    let baseline = match workspace::prepare(&guard.seed().workspace) {
        Ok(value) => value,
        Err(error) => return guard.fail(SettlementCause::Repository, &error),
    };
    guard.seed_mut().baseline = Some(baseline.clone());

    let cancellation = CancellationToken::new();
    let authenticated = match providers::authenticated(&cancellation).await {
        Ok(value) => value,
        Err(error) => return guard.fail(SettlementCause::Provider, &error),
    };
    guard.seed_mut().provider_routes = authenticated.routes;
    let (observer, observations) = observation_capture();
    let command_runtime = match crate::command_runtime::open(&baseline.root, guard.seed().run_id) {
        Ok(value) => value,
        Err(error) => return guard.fail(SettlementCause::Repository, &error),
    };
    let result = ProductRunner::run(
        ProductRunInput {
            run_id: guard.seed().run_id,
            workspace_root: baseline.root.clone(),
            trace_path: guard.seed().trace_path.clone(),
            command_runtime,
            finding_state: String::new(),
            task: prompt,
            max_elapsed,
            delivery_scope,
            conversation: Arc::new(conversation.clone()),
            providers: authenticated.roles,
            cancelled: Arc::new(AtomicBool::new(false)),
            provider_cancellation: cancellation,
        },
        observer,
    )
    .await;

    let snapshot = match candidate::capture(&baseline.root, Some(&baseline.head)) {
        Ok(value) => value,
        Err(error) => return guard.fail(SettlementCause::Adapter, &error),
    };
    if let Err(error) =
        retain_evidence(&mut guard, &conversation, &observations, &evidence_dir, sandbox.as_deref())
    {
        return guard.finalize(crate::settlement::TerminalFacts::failure(
            SettlementCause::Adapter,
            Some(snapshot),
            &error,
        ));
    }
    settle_product_result(&mut guard, result, snapshot)
}

fn retain_evidence(
    guard: &mut crate::settlement::InvocationGuard,
    conversation: &crate::session::BenchmarkSession,
    observations: &ObservationCapture,
    evidence_dir: &Path,
    sandbox: Option<&Path>,
) -> Result<(), BenchmarkError> {
    let last_update = observations.lock().ok().and_then(|mut value| value.take());
    if let Some(update) = last_update {
        guard.seed_mut().resources = ResourceReport::from(update.progress);
        guard.seed_mut().last_observation_path =
            Some(ProductObservation::from_update(update).publish(evidence_dir)?);
    }
    let trace_inputs = conversation.trace_inputs();
    guard.seed_mut().session_trace_paths =
        trace_inputs.iter().map(|(path, _)| path.clone()).collect();
    match guard.seed().suite {
        BenchmarkSuite::HarnessBench => retain_harness_evidence(guard, &trace_inputs, sandbox),
        BenchmarkSuite::TerminalBench => {
            guard.seed_mut().usage =
                trace::summarize_usage(&guard.seed().trace_path, &conversation.render())?;
            Ok(())
        }
    }
}

fn retain_harness_evidence(
    guard: &mut crate::settlement::InvocationGuard,
    trace_inputs: &[(PathBuf, String)],
    sandbox: Option<&Path>,
) -> Result<(), BenchmarkError> {
    let usage_proxy = guard.seed().usage_proxy.clone().ok_or_else(|| {
        BenchmarkError::Workspace("Harness-Bench usage proxy was not admitted".to_owned())
    })?;
    guard.seed_mut().projected_responses = trace::publish_harnessbench(
        trace_inputs,
        &usage_proxy,
        &guard.seed().task_id,
        &guard.seed().session_id,
        &guard.seed().harness_model_id,
    )?;
    guard.seed_mut().usage = trace::summarize_usage(
        &guard.seed().trace_path,
        &trace_inputs.last().map_or_else(String::new, |(_, prompt)| prompt.clone()),
    )?;
    let sandbox = sandbox.ok_or_else(|| {
        BenchmarkError::Workspace("Harness-Bench sandbox was not admitted".to_owned())
    })?;
    guard.seed_mut().relocatable_paths = Some(RelocatablePaths::new(
        sandbox,
        &guard.seed().workspace,
        &guard.seed().trace_path,
        &guard.seed().session_trace_paths,
        &usage_proxy,
        guard.seed().last_observation_path.as_deref(),
    )?);
    Ok(())
}

fn settle_product_result(
    guard: &mut crate::settlement::InvocationGuard,
    result: Result<ProductRunOutcome, ProductRunnerError>,
    snapshot: candidate::CandidateSnapshot,
) -> Result<crate::evidence::InvocationReport, BenchmarkError> {
    match result {
        Ok(ProductRunOutcome::Complete(output)) => {
            guard.finalize(crate::settlement::TerminalFacts {
                cause: SettlementCause::Completed,
                snapshot: Some(snapshot),
                qualified: true,
                qualification: QualificationReport::accepted(output.gates, output.review),
                summary: Some(output.summary),
                failure_kind: None,
                failure: None,
            })
        }
        Ok(ProductRunOutcome::WaitingForUser { question, .. }) => {
            let snapshot = (!snapshot.changed_paths.is_empty()).then_some(snapshot);
            guard.finalize(crate::settlement::TerminalFacts {
                cause: SettlementCause::UserWait,
                snapshot,
                qualified: false,
                qualification: QualificationReport::candidate(
                    "changed",
                    observation_text(guard, true),
                    observation_text(guard, false),
                ),
                summary: None,
                failure_kind: Some("waiting_for_user".to_owned()),
                failure: Some(question),
            })
        }
        Err(error) => {
            let cause = product_error_cause(&error);
            let snapshot = (!snapshot.changed_paths.is_empty()).then_some(snapshot);
            let benchmark_error = BenchmarkError::Workspace(error.to_string());
            let mut facts =
                crate::settlement::TerminalFacts::failure(cause, snapshot, &benchmark_error);
            facts.failure_kind = Some(format!("{:?}", error.kind()).to_lowercase());
            guard.finalize(facts)
        }
    }
}

fn observation_text(guard: &crate::settlement::InvocationGuard, gates: bool) -> Option<String> {
    let path = guard.seed().last_observation_path.as_ref()?;
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    value[if gates { "gates" } else { "review" }].as_str().map(str::to_owned)
}

fn product_error_cause(error: &ProductRunnerError) -> SettlementCause {
    let detail = error.detail().to_ascii_lowercase();
    if detail.contains("context window") || detail.contains("context limit") {
        return SettlementCause::Context;
    }
    if matches!(error.kind(), ProductRunnerErrorKind::Budget)
        && (detail.contains("elapsed") || detail.contains("deadline") || detail.contains("horizon"))
    {
        return SettlementCause::Deadline;
    }
    match error.kind() {
        ProductRunnerErrorKind::Repository => SettlementCause::Repository,
        ProductRunnerErrorKind::Provider => SettlementCause::Provider,
        ProductRunnerErrorKind::Gate => SettlementCause::Gate,
        ProductRunnerErrorKind::Cancelled => SettlementCause::Cancellation,
        ProductRunnerErrorKind::InvalidModelOutput
            if error.operation().to_ascii_lowercase().contains("review") =>
        {
            SettlementCause::Review
        }
        ProductRunnerErrorKind::InvalidModelOutput
        | ProductRunnerErrorKind::Apply
        | ProductRunnerErrorKind::Budget => SettlementCause::Adapter,
    }
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
