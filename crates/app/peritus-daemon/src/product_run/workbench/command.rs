//! Workspace availability, permission and negotiated capability admission for control commands.

use super::mapping::domain_operation;
use super::{
    ActorId, AppResponsePayload, ControlError, ProductRunService, WorkbenchCommand,
    WorkbenchIntent, WorkbenchReceipt, domain_operation_with_store, error_response, guidance,
    receipt_projection, resolve_user_operation, review,
};
use crate::product_control::{AuthorityKey, AuthoritySet};
use crate::product_run::permissions;
use peritus_product_runner::control::ConversationId;

impl ProductRunService {
    pub(crate) async fn workbench_command(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        self.workbench_command_negotiated(actor, command, true).await
    }

    pub(crate) async fn workbench_command_negotiated(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
        checkpoint_coverage: bool,
    ) -> AppResponsePayload {
        self.workbench_command_with_checkpoint_features(actor, command, checkpoint_coverage, true)
            .await
    }

    pub(crate) async fn workbench_command_with_checkpoint_features(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
        checkpoint_coverage: bool,
        checkpoint_manifests: bool,
    ) -> AppResponsePayload {
        let service = self.clone();
        let command = command.clone();
        Self::await_blocking_future("run durable workbench command", move || async move {
            service
                .workbench_command_with_checkpoint_features_owned(
                    actor,
                    command,
                    checkpoint_coverage,
                    checkpoint_manifests,
                )
                .await
        })
        .await
        .unwrap_or_else(crate::product_run::ProductRunServiceError::response)
    }

    async fn workbench_command_with_checkpoint_features_owned(
        &self,
        actor: ActorId,
        command: WorkbenchCommand,
        checkpoint_coverage: bool,
        checkpoint_manifests: bool,
    ) -> AppResponsePayload {
        if matches!(command.intent(), WorkbenchIntent::Queue(queue) if super::inputs::is_source_intent(queue)) {
            return error_response(ControlError::InvalidInput.into());
        }
        if let Err(error) = self.ensure_workspace_available(command.query().workspace()) {
            return error.response();
        }
        let required = permissions::command_permissions(command.intent());
        if !required.is_empty()
            && let Err(error) = self.require_workspace_permissions(actor, command.query(), required)
        {
            return error_response(error);
        }
        match command.intent() {
            WorkbenchIntent::StartExecution(_) | WorkbenchIntent::StartGoal { .. } => {
                return self.start_workbench(actor, &command).await;
            }
            WorkbenchIntent::ContinueExecution(_) => {
                return self.continue_workbench_execution(actor, &command).await;
            }
            WorkbenchIntent::CreateCheckpoint(_) => {
                return self
                    .create_workbench_checkpoint(
                        actor,
                        &command,
                        checkpoint_coverage,
                        checkpoint_manifests,
                    )
                    .await;
            }
            _ => {}
        }
        if matches!(
            command.intent(),
            WorkbenchIntent::StartPreview(_)
                | WorkbenchIntent::InteractPreview { .. }
                | WorkbenchIntent::StopPreview { .. }
                | WorkbenchIntent::CheckPreviewBehavior { .. }
                | WorkbenchIntent::AddArtifactFeedback { .. }
        ) {
            return self.workbench_preview_local_command(actor, &command);
        }
        if matches!(command.intent(), WorkbenchIntent::ResumeGoal { .. }) {
            return self.resume_workbench_goal(actor, &command).await;
        }
        let review = matches!(
            command.intent(),
            WorkbenchIntent::AddReview { .. }
                | WorkbenchIntent::RebindReview { .. }
                | WorkbenchIntent::DismissReview { .. }
        );
        if review {
            let operation = match domain_operation(actor, &command) {
                Ok(operation) => operation,
                Err(error) => return error_response(error),
            };
            match self.with_control_conversation(operation.conversation(), |store| {
                store.resolve(&operation)
            }) {
                Ok(Some(receipt)) => {
                    return WorkbenchReceipt::new(
                        command.operation(),
                        command.query(),
                        receipt.accepted_revision(),
                        receipt.payload_digest(),
                    )
                    .map_or_else(
                        |_| error_response(ControlError::InvalidInput.into()),
                        AppResponsePayload::WorkbenchReceipt,
                    );
                }
                Ok(None) => {}
                Err(error) => return error_response(error),
            }
            if let Err(error) = review::validate_command(self, actor, &command) {
                return error_response(error);
            }
        }
        if matches!(command.intent(), WorkbenchIntent::ForkConversation(_)) {
            return self.fork_workbench(actor, &command);
        }
        if guidance::is_guidance(command.intent()) {
            return self.workbench_guidance_command(actor, &command);
        }
        let result = self.control_workspace(command.query()).and_then(|()| {
            let conversation =
                ConversationId::new(command.query().conversation().into_bytes())?;
            let permission_host = if matches!(command.intent(), WorkbenchIntent::SetPermissions(_))
            {
                Some(self.permission_host(command.query().workspace())?)
            } else {
                None
            };
            let authorities = if permission_host.is_some() {
                AuthoritySet::new([
                    AuthorityKey::Conversation(conversation),
                    AuthorityKey::Workspace(command.query().workspace()),
                ])
            } else {
                AuthoritySet::new([AuthorityKey::Conversation(conversation)])
            };
            self.with_control_authorities(authorities, |store| {
                let operation = domain_operation_with_store(store, actor, &command)?;
                if let Some(receipt) = resolve_user_operation(store, &operation)? {
                    return Ok((receipt, true));
                }
                match permission_host {
                    Some(host) => store.accept_permissions(&operation, host),
                    None => store.accept(&operation),
                }
                .map(|receipt| (receipt, false))
            })
            .and_then(|(receipt, replay)| {
                receipt_projection(&command, &receipt).map(|projected| (projected, replay))
            })
        });
        let fresh = result.as_ref().is_ok_and(|(_, replay)| !replay);
        let response = result
            .map(|(receipt, _)| receipt)
            .map_or_else(error_response, AppResponsePayload::WorkbenchReceipt);
        if matches!(
            command.intent(),
            WorkbenchIntent::PauseGoal {
                mode: peritus_app_protocol::WorkbenchGoalPauseMode::Now,
                ..
            } | WorkbenchIntent::ClearGoal { .. }
        ) && fresh
            && matches!(response, AppResponsePayload::WorkbenchReceipt(_))
        {
            self.signal_goal_cancellation(&command);
        }
        if review
            && fresh
            && matches!(response, AppResponsePayload::WorkbenchReceipt(_))
            && let Err(error) = review::resume_feedback(self, actor, &command).await
        {
            return error.response();
        }
        response
    }
}
