//! Active product-run task ownership and terminal projection.

use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

use peritus_app_protocol::{
    ProductConversationRole, ProductDeliverable, ProductRunPhase, ProductRunRequest,
    ProductRunSnapshot,
};
use peritus_product_runner::{
    CommandRuntime, ConversationView, PRODUCT_RUN_MAX_ELAPSED, ProductDeliveryScope,
    ProductRunInput, ProductRunOutcome, ProductRunner, RoleProviders, RunObserver,
};
use peritus_provider_core::CancellationToken;
use peritus_run_settlement::RunDisposition;
use peritus_types::RunId;

use super::persistence::persist_record;
use super::snapshot::replace_snapshot;
use super::{ProductRunService, SharedConversation};

impl ProductRunService {
    #[allow(
        clippy::too_many_arguments,
        reason = "daemon execution inputs stay explicit at the task ownership boundary"
    )]
    pub(super) async fn spawn(
        &self,
        request: ProductRunRequest,
        workspace_root: PathBuf,
        providers: RoleProviders,
        cancelled: Arc<AtomicBool>,
        provider_cancellation: CancellationToken,
        conversation: Arc<SharedConversation>,
        finding_state: String,
    ) {
        let service = self.clone();
        let run_id = request.run_id();
        let trace_path = self.inner.directory.join(format!("{}.trace", run_hex(run_id)));
        let observer: RunObserver = Arc::new(move |update| service.observe(run_id, update));
        let service = self.clone();
        let task = tokio::spawn(async move {
            let command_runtime = match CommandRuntime::open(
                service.inner.directory.join("commands").join(run_hex(run_id)),
                &workspace_root,
                run_id,
                service.inner.processes.clone(),
            ) {
                Ok(runtime) => runtime,
                Err(error) => {
                    service.finish(run_id, Err(error));
                    return;
                }
            };
            let conversation: Arc<dyn ConversationView> = conversation;
            let result = ProductRunner::run(
                ProductRunInput {
                    run_id,
                    workspace_id: request.workspace_id(),
                    workspace_root,
                    trace_path,
                    command_runtime,
                    finding_state,
                    task: request.task().to_owned(),
                    max_elapsed: PRODUCT_RUN_MAX_ELAPSED,
                    delivery_scope: ProductDeliveryScope::WorkspaceChanges,
                    conversation,
                    providers,
                    cancelled,
                    provider_cancellation,
                    resume: None,
                },
                observer,
            )
            .await;
            service.finish(run_id, result);
        });
        let mut tasks = self.inner.tasks.lock().await;
        tasks.retain(|existing| !existing.is_finished());
        tasks.push(task);
    }

    fn observe(&self, run_id: RunId, update: peritus_product_runner::ProductRunUpdate) {
        let Ok(mut records) = self.inner.records.write() else { return };
        let Some(record) = records.get_mut(&run_id) else { return };
        if record.snapshot.phase() == ProductRunPhase::RecoveryRequired {
            return;
        }
        if !update.finding_state.is_empty() {
            record.finding_state = update.finding_state;
        }
        record.progress.observe(update.progress);
        let phase = match update.phase {
            peritus_product_runner::ProductRunPhase::Designing => ProductRunPhase::Designing,
            peritus_product_runner::ProductRunPhase::Writing => ProductRunPhase::Writing,
            peritus_product_runner::ProductRunPhase::Checking => ProductRunPhase::Checking,
            peritus_product_runner::ProductRunPhase::Reviewing => ProductRunPhase::Reviewing,
            peritus_product_runner::ProductRunPhase::Fixing => ProductRunPhase::Fixing,
            peritus_product_runner::ProductRunPhase::Verifying => ProductRunPhase::Verifying,
            peritus_product_runner::ProductRunPhase::Finalizing => ProductRunPhase::Verifying,
            peritus_product_runner::ProductRunPhase::Complete => ProductRunPhase::Complete,
        };
        if let Ok(snapshot) = ProductRunSnapshot::new(
            run_id,
            record.request.workspace_id(),
            record.request.providers(),
            phase,
            update.cycle,
            record.request.task().to_owned(),
            update.status,
            update.diff,
            update.gates,
            update.review,
            update.summary,
        ) {
            record.snapshot = snapshot;
            let _ = persist_record(&self.inner.directory, record);
        }
    }

    fn finish(
        &self,
        run_id: RunId,
        result: Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>,
    ) {
        let Ok(mut records) = self.inner.records.write() else { return };
        let Some(record) = records.get_mut(&run_id) else { return };
        if record.snapshot.phase() == ProductRunPhase::RecoveryRequired {
            let _ = persist_record(&self.inner.directory, record);
            return;
        }
        match result {
            Ok(outcome) if outcome.settlement().disposition() == RunDisposition::Accepted => {
                let Some(output) = outcome.candidate() else {
                    fail_handoff(record);
                    let _ = persist_record(&self.inner.directory, record);
                    return;
                };
                let completion_message = format!("Completed: {}", output.summary);
                let deliverable = self.project_deliverable(record, &outcome);
                let Some(deliverable) = deliverable else {
                    fail_handoff(record);
                    let _ = persist_record(&self.inner.directory, record);
                    return;
                };
                if let Ok(snapshot) = ProductRunSnapshot::new(
                    run_id,
                    record.request.workspace_id(),
                    record.request.providers(),
                    ProductRunPhase::Complete,
                    output.fixer_cycles + 1,
                    record.request.task().to_owned(),
                    "Run completed with passing checks".to_owned(),
                    output.diff.clone(),
                    output.gates.clone(),
                    output.review.clone(),
                    output.summary.clone(),
                ) {
                    record.snapshot = snapshot.with_deliverable(deliverable);
                } else {
                    fail_handoff(record);
                    let _ = persist_record(&self.inner.directory, record);
                    return;
                }
                let _ =
                    record.conversation.append(ProductConversationRole::Agent, completion_message);
            }
            Ok(outcome) if outcome.settlement().disposition() == RunDisposition::WaitingForUser => {
                let Some(question) = outcome.question() else {
                    fail_handoff(record);
                    let _ = persist_record(&self.inner.directory, record);
                    return;
                };
                let _ = record
                    .conversation
                    .append(ProductConversationRole::Agent, question.message().to_owned());
                if let Ok(snapshot) = replace_snapshot(
                    &record.snapshot,
                    ProductRunPhase::WaitingForUser,
                    "Waiting for your reply",
                    question.message(),
                ) {
                    record.snapshot = self.with_candidate(record, &outcome, snapshot);
                }
            }
            Ok(outcome) => self.finish_nonaccepted(record, &outcome),
            Err(error) => {
                let phase =
                    if error.kind() == peritus_product_runner::ProductRunnerErrorKind::Cancelled {
                        ProductRunPhase::Cancelled
                    } else {
                        ProductRunPhase::Failed
                    };
                if let Ok(snapshot) = replace_snapshot(
                    &record.snapshot,
                    phase,
                    &format!("{} failed", error.operation()),
                    error.detail(),
                ) {
                    record.snapshot = snapshot;
                }
                let _ = record.conversation.append(
                    ProductConversationRole::Agent,
                    format!(
                        "I couldn't finish this run: {}: {}. Send a message to correct, clarify, or continue it.",
                        error.operation(),
                        error.detail()
                    ),
                );
            }
        }
        let _ = persist_record(&self.inner.directory, record);
    }

    fn finish_nonaccepted(&self, record: &mut super::RunRecord, outcome: &ProductRunOutcome) {
        let phase = match outcome.settlement().disposition() {
            RunDisposition::Cancelled => ProductRunPhase::Cancelled,
            RunDisposition::RecoveryRequired => ProductRunPhase::RecoveryRequired,
            RunDisposition::CandidateAvailable | RunDisposition::FailedNoCandidate => {
                ProductRunPhase::Failed
            }
            RunDisposition::Accepted | RunDisposition::WaitingForUser => return,
        };
        let status = match outcome.settlement().disposition() {
            RunDisposition::CandidateAvailable => "Run stopped with a candidate available",
            RunDisposition::RecoveryRequired => "Run requires recovery before it can continue",
            RunDisposition::Cancelled => "Run cancelled",
            _ => "Run stopped before producing a candidate",
        };
        let detail = outcome.detail().unwrap_or(status);
        if let Ok(snapshot) = replace_snapshot(&record.snapshot, phase, status, detail) {
            record.snapshot = self.with_candidate(record, outcome, snapshot);
        }
        let remaining = if outcome.remaining_work().is_empty() {
            String::new()
        } else {
            format!(" Remaining work: {}.", outcome.remaining_work().join("; "))
        };
        let _ = record
            .conversation
            .append(ProductConversationRole::Agent, format!("{status}: {detail}.{remaining}"));
    }

    fn with_candidate(
        &self,
        record: &super::RunRecord,
        outcome: &ProductRunOutcome,
        snapshot: ProductRunSnapshot,
    ) -> ProductRunSnapshot {
        match self.project_deliverable(record, outcome) {
            Some(deliverable) => snapshot.with_deliverable(deliverable),
            None => snapshot,
        }
    }

    fn project_deliverable(
        &self,
        record: &super::RunRecord,
        outcome: &ProductRunOutcome,
    ) -> Option<ProductDeliverable> {
        let output = outcome.candidate()?;
        let stage = outcome.settlement().checkpoint()?.stage();
        let workspace = self.inner.workspaces.get(&record.request.workspace_id())?;
        ProductDeliverable::candidate(
            workspace.to_string_lossy().into_owned(),
            output.changed_paths.iter().map(|path| path.to_string_lossy().into_owned()).collect(),
            output.successful_commands.clone(),
            output.run_instructions.clone(),
            stage,
        )
        .ok()
    }
}

fn fail_handoff(record: &mut super::RunRecord) {
    let detail = "Passing checks could not be projected into a durable deliverable handoff";
    if let Ok(snapshot) = replace_snapshot(
        &record.snapshot,
        ProductRunPhase::Failed,
        "Create durable deliverable handoff failed",
        detail,
    ) {
        record.snapshot = snapshot;
    }
    let _ = record.conversation.append(ProductConversationRole::Agent, detail.to_owned());
}

fn run_hex(run_id: RunId) -> String {
    run_id.as_bytes().iter().fold(String::new(), |mut value, byte| {
        use core::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
        value
    })
}
