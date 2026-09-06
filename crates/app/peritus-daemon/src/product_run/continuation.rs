//! Durable follow-up admission and idle-boundary restart.
use super::{
    ProductRunService, ProductRunServiceError, RunProgress, initial_snapshot, interaction,
    persist_record, replace_snapshot, workspace_has_active_run,
};
use peritus_app_protocol::{
    ProductConversationRole, ProductRunContinuation, ProductRunPhase, ProductRunSnapshot,
};
use peritus_provider_core::CancellationToken;
use std::sync::{Arc, atomic::AtomicBool};

impl ProductRunService {
    pub(crate) async fn continue_run(
        &self,
        continuation: &ProductRunContinuation,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        self.continue_configured(continuation, None).await
    }

    pub(super) async fn continue_configured(
        &self,
        continuation: &ProductRunContinuation,
        options: Option<interaction::InteractionOptions>,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        let mut restart = None;
        let snapshot = {
            let mut records =
                self.inner.records.write().map_err(|_| ProductRunServiceError::Unavailable)?;
            let workspace_id = records
                .get(&continuation.run_id())
                .ok_or(ProductRunServiceError::NotFound)?
                .request
                .workspace_id();
            let was_terminal = records
                .get(&continuation.run_id())
                .expect("checked product run exists")
                .snapshot
                .phase()
                .terminal();
            if was_terminal
                && workspace_has_active_run(&records, workspace_id, Some(continuation.run_id()))
            {
                return Err(ProductRunServiceError::InvalidState);
            }
            let record =
                records.get_mut(&continuation.run_id()).expect("checked product run exists");
            // Resolve a new adapter before changing durable input or selected options. A bad
            // manual model must not partially admit a follow-up into the live record.
            let resolved = if was_terminal {
                Some(self.resolve_selected_providers(
                    record.request.providers(),
                    options.as_ref().or(record.interaction.as_ref()),
                )?)
            } else {
                None
            };
            if let Some(options) = options {
                let prior =
                    record.interaction.as_mut().ok_or(ProductRunServiceError::InvalidState)?;
                if !was_terminal && (prior.mode != options.mode || prior.models != options.models) {
                    return Err(ProductRunServiceError::InvalidState);
                }
                prior.mode = options.mode;
                prior.models = options.models;
            }
            record
                .conversation
                .append(ProductConversationRole::User, continuation.message().to_owned())?;
            if let Some(options) = record.interaction.as_mut() {
                use peritus_product_runner::ConversationView as _;
                options.append(
                    peritus_app_protocol::ProductActivityKind::User,
                    continuation.message(),
                    &format!("Input {} received", record.conversation.revision()),
                )?;
            }
            if was_terminal {
                let providers = resolved.ok_or(ProductRunServiceError::Unavailable)?;
                let root = self
                    .inner
                    .workspaces
                    .get(&record.request.workspace_id())
                    .cloned()
                    .ok_or(ProductRunServiceError::WorkspaceUnavailable)?;
                let cancelled = Arc::new(AtomicBool::new(false));
                let token = CancellationToken::new();
                record.cancelled = Arc::clone(&cancelled);
                record.provider_cancellation = token.clone();
                // A follow-up changes the governing conversation revision, so the prior
                // deliverable and its qualification cannot be projected as current while the
                // replacement run is active. The checkpoint and resume state remain durable for
                // phase planning and failure settlement.
                record.snapshot = initial_snapshot(&record.request)?;
                record.snapshot = replace_snapshot(
                    &record.snapshot,
                    ProductRunPhase::Queued,
                    "Follow-up queued for the writer",
                    "",
                )?;
                record.progress = RunProgress::default();
                record.settlement = None;
                record.interruption_cause.clear();
                restart = Some((
                    record.request.clone(),
                    root,
                    providers,
                    cancelled,
                    token,
                    Arc::clone(&record.conversation),
                    record.finding_state.clone(),
                    record.resume.clone(),
                ));
            } else {
                record.snapshot = replace_snapshot(
                    &record.snapshot,
                    record.snapshot.phase(),
                    "Follow-up received; the next model step will incorporate it",
                    record.snapshot.summary(),
                )?;
            }
            persist_record(&self.inner.directory, record)?;
            record.snapshot.clone()
        };
        if let Some((
            request,
            root,
            providers,
            cancelled,
            token,
            conversation,
            finding_state,
            resume,
        )) = restart
        {
            self.spawn(
                request,
                root,
                providers,
                cancelled,
                token,
                conversation,
                finding_state,
                resume,
            )
            .await;
        }
        Ok(snapshot)
    }
}
