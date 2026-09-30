//! Durable follow-up admission and idle-boundary restart.
use super::{
    ProductRunService, ProductRunServiceError, RunProgress, initial_snapshot, interaction,
    replace_snapshot, workspace_has_active_run,
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
        if self.governed_run(continuation.run_id())? {
            return Err(ProductRunServiceError::Control(
                peritus_product_runner::control::ControlError::UnsupportedSchema,
            ));
        }
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
            super::deliverable::discard::workspace_available(
                &self.inner.directory,
                &records,
                workspace_id,
            )?;
            let record = records
                .get_mut(&continuation.run_id())
                .ok_or(ProductRunServiceError::InvalidState)?;
            let mut next = record.clone();
            self.validate_workspace_mode(
                workspace_id,
                options.as_ref().or(next.interaction.as_ref()),
            )?;
            // Resolve a new adapter before changing durable input or selected options. A bad
            // manual model must not partially admit a follow-up into the live record.
            let resolved = if was_terminal {
                Some(self.resolve_selected_providers(
                    record.request.providers(),
                    options.as_ref().or(next.interaction.as_ref()),
                )?)
            } else {
                None
            };
            if let Some(options) = options {
                let prior =
                    next.interaction.as_mut().ok_or(ProductRunServiceError::InvalidState)?;
                if !was_terminal && (prior.mode != options.mode || prior.models != options.models) {
                    return Err(ProductRunServiceError::InvalidState);
                }
                prior.mode = options.mode;
                prior.models = options.models;
            }
            next.conversation = next
                .conversation
                .appended(ProductConversationRole::User, continuation.message().to_owned())?;
            if let Some(options) = next.interaction.as_mut() {
                use peritus_product_runner::ConversationView as _;
                options.append(
                    peritus_app_protocol::ProductActivityKind::User,
                    continuation.message(),
                    &format!("Input {} received", next.conversation.revision()),
                )?;
            }
            if was_terminal {
                let providers = resolved.ok_or(ProductRunServiceError::Unavailable)?;
                let root = self
                    .inner
                    .workspaces
                    .get(&next.request.workspace_id())
                    .cloned()
                    .ok_or(ProductRunServiceError::WorkspaceUnavailable)?;
                let cancelled = Arc::new(AtomicBool::new(false));
                let token = CancellationToken::new();
                next.cancelled = Arc::clone(&cancelled);
                next.user_cancelled = false;
                next.provider_cancellation = token.clone();
                // A follow-up changes the governing conversation revision, so the prior
                // deliverable and its qualification cannot be projected as current while the
                // replacement run is active. The checkpoint and resume state remain durable for
                // phase planning and failure settlement.
                next.snapshot = initial_snapshot(&next.request)?;
                next.snapshot = replace_snapshot(
                    &next.snapshot,
                    ProductRunPhase::Queued,
                    "Follow-up queued for the writer",
                    "",
                )?;
                next.progress = RunProgress::default();
                next.settlement = None;
                next.interruption_cause.clear();
                restart = Some((
                    next.request.clone(),
                    root,
                    providers,
                    cancelled,
                    token,
                    Arc::clone(&next.conversation),
                    next.finding_state.clone(),
                    next.resume.clone(),
                ));
            } else {
                next.snapshot = replace_snapshot(
                    &next.snapshot,
                    next.snapshot.phase(),
                    "Follow-up received; the next model step will incorporate it",
                    next.snapshot.summary(),
                )?;
            }
            super::persistence::write_record(&self.inner.directory, &next)?;
            let snapshot = next.snapshot.clone();
            *record = next;
            snapshot
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
