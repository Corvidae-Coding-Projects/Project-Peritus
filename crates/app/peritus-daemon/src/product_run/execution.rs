//! Active product-run task ownership and terminal projection.

use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

use peritus_app_protocol::{
    ProductConversationRole, ProductRunPhase, ProductRunRequest, ProductRunSnapshot,
};
use peritus_product_runner::{
    ConversationView, ProductRunInput, ProductRunOutcome, ProductRunner, RoleProviders, RunObserver,
};
use peritus_provider_core::CancellationToken;
use peritus_types::RunId;

use super::persistence::persist_record;
use super::snapshot::replace_snapshot;
use super::{ProductRunService, SharedConversation};

impl ProductRunService {
    pub(super) async fn spawn(
        &self,
        request: ProductRunRequest,
        workspace_root: PathBuf,
        providers: RoleProviders,
        cancelled: Arc<AtomicBool>,
        provider_cancellation: CancellationToken,
        conversation: Arc<SharedConversation>,
    ) {
        let service = self.clone();
        let run_id = request.run_id();
        let observer: RunObserver = Arc::new(move |update| service.observe(run_id, update));
        let service = self.clone();
        let task = tokio::spawn(async move {
            let conversation: Arc<dyn ConversationView> = conversation;
            let result = ProductRunner::run(
                ProductRunInput {
                    run_id,
                    workspace_root,
                    task: request.task().to_owned(),
                    conversation,
                    providers,
                    cancelled,
                    provider_cancellation,
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
        let phase = match update.phase {
            peritus_product_runner::ProductRunPhase::Writing => ProductRunPhase::Writing,
            peritus_product_runner::ProductRunPhase::Checking => ProductRunPhase::Checking,
            peritus_product_runner::ProductRunPhase::Reviewing => ProductRunPhase::Reviewing,
            peritus_product_runner::ProductRunPhase::Fixing => ProductRunPhase::Fixing,
            peritus_product_runner::ProductRunPhase::Verifying => ProductRunPhase::Verifying,
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
        match result {
            Ok(ProductRunOutcome::Complete(output)) => {
                let completion_message = format!("Completed: {}", output.summary);
                if let Ok(snapshot) = ProductRunSnapshot::new(
                    run_id,
                    record.request.workspace_id(),
                    record.request.providers(),
                    ProductRunPhase::Complete,
                    output.fixer_cycles + 1,
                    record.request.task().to_owned(),
                    "Run completed with passing checks".to_owned(),
                    output.diff,
                    output.gates,
                    output.review,
                    output.summary,
                ) {
                    record.snapshot = snapshot;
                }
                let _ =
                    record.conversation.append(ProductConversationRole::Agent, completion_message);
            }
            Ok(ProductRunOutcome::WaitingForUser { question, .. }) => {
                let _ =
                    record.conversation.append(ProductConversationRole::Agent, question.clone());
                if let Ok(snapshot) = replace_snapshot(
                    &record.snapshot,
                    ProductRunPhase::WaitingForUser,
                    "Waiting for your reply",
                    &question,
                ) {
                    record.snapshot = snapshot;
                }
            }
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
}
