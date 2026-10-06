//! Workspace availability, permission and negotiated capability admission for control commands.

use super::mapping::domain_operation;
use super::{
    ActorId, AppResponsePayload, ControlError, ProductRunService, WorkbenchCommand,
    WorkbenchIntent, WorkbenchReceipt, domain_operation_with_store, error_response, guidance,
    receipt_projection, resolve_user_operation, review,
};
use crate::product_run::permissions;

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
                return self.start_workbench(actor, command).await;
            }
            WorkbenchIntent::CreateCheckpoint(_) => {
                return self.create_workbench_checkpoint(actor, command, checkpoint_coverage).await;
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
            return self.workbench_preview_local_command(actor, command);
        }
        if matches!(command.intent(), WorkbenchIntent::ResumeGoal { .. }) {
            return self.resume_workbench_goal(actor, command).await;
        }
        let review = matches!(
            command.intent(),
            WorkbenchIntent::AddReview { .. }
                | WorkbenchIntent::RebindReview { .. }
                | WorkbenchIntent::DismissReview { .. }
        );
        if review {
            let operation = match domain_operation(actor, command) {
                Ok(operation) => operation,
                Err(error) => return error_response(error),
            };
            match self.with_controls(false, |store| store.resolve(&operation)) {
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
            if let Err(error) = review::validate_command(self, actor, command) {
                return error_response(error);
            }
        }
        if matches!(command.intent(), WorkbenchIntent::ForkConversation(_)) {
            return self.fork_workbench(actor, command);
        }
        if guidance::is_guidance(command.intent()) {
            return self.workbench_guidance_command(actor, command);
        }
        let result = self.control_workspace(command.query()).and_then(|()| {
            let create = matches!(command.intent(), WorkbenchIntent::CreateConversation(_));
            let permission_host = if matches!(command.intent(), WorkbenchIntent::SetPermissions(_))
            {
                Some(self.permission_host(command.query().workspace())?)
            } else {
                None
            };
            self.with_controls(create, |store| {
                let operation = domain_operation_with_store(store, actor, command)?;
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
                receipt_projection(command, &receipt).map(|projected| (projected, replay))
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
            self.signal_goal_cancellation(command);
        }
        if review
            && fresh
            && matches!(response, AppResponsePayload::WorkbenchReceipt(_))
            && let Err(error) = review::resume_feedback(self, actor, command).await
        {
            return error.response();
        }
        response
    }
}
