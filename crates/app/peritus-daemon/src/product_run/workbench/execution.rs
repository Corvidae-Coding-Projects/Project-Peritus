//! Explicit fresh execution admission. Legacy state is never implicitly upgraded or resumed.

use super::{
    ProductRunService, domain_operation, error_response, receipt_projection, resolve_user_operation,
};
use crate::product_run::{
    ProductRunRequest, ProductRunServiceError, interaction::InteractionOptions,
};
use peritus_app_protocol::{AppResponsePayload, WorkbenchCommand, WorkbenchIntent};
use peritus_product_runner::control::{ControlError, ControlReceipt, ConversationRecord};
use peritus_types::ActorId;

enum Admission {
    Existing(ControlReceipt),
    Fresh { title: String },
}

impl ProductRunService {
    pub(super) async fn start_workbench(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        let settings = match command.intent() {
            WorkbenchIntent::StartExecution(settings)
            | WorkbenchIntent::StartGoal { settings, .. } => settings,
            _ => return ProductRunServiceError::InvalidMessage.response(),
        };
        let operation = match self
            .control_workspace(command.query())
            .and_then(|()| domain_operation(actor, command))
        {
            Ok(operation) => operation,
            Err(error) => return error_response(error),
        };
        let admission = self.with_controls(false, |store| {
            if let Some(receipt) = resolve_user_operation(store, &operation)? {
                return Ok(Admission::Existing(receipt));
            }
            let current = store.load(operation.conversation())?.ok_or(ControlError::NotFound)?;
            if let Some(branch) = store.branch(operation.conversation())?
                && !branch_execution_allowed(&branch, operation.intent(), settings.mode())
            {
                return Err(ControlError::InvalidInput.into());
            }
            // Pure preflight before creating any run record; the actual publication checks CAS again.
            ConversationRecord::apply(Some(&current), &operation)?;
            Ok(Admission::Fresh { title: current.title().to_owned() })
        });
        let title = match admission {
            Ok(Admission::Existing(receipt)) => {
                return receipt_projection(command, &receipt)
                    .map_or_else(error_response, AppResponsePayload::WorkbenchReceipt);
            }
            Ok(Admission::Fresh { title }) => title,
            Err(error) => return error_response(error),
        };
        // All governing user content comes from the captured ledger at D0's exact request boundary.
        // The conversation title is presentation metadata; the fixed execution directive prevents
        // renamed, withdrawn, or edited display text from becoming model input.
        let request = match ProductRunRequest::new(
            settings.run(),
            command.query().workspace(),
            settings.providers(),
            "Execute the selected durable workbench inputs.".to_owned(),
        )
        .and_then(|request| request.with_display_task(title))
        {
            Ok(request) => request,
            Err(_) => return ProductRunServiceError::InvalidMessage.response(),
        };
        let options =
            InteractionOptions::new(operation.clone(), settings.mode(), settings.models().clone());
        if let Err(error) = self.validate_models(settings.providers(), &options.models).await {
            return error.response();
        }
        match self.start_configured(request, options).await {
            Ok(_) => self
                .with_controls(false, |store| {
                    resolve_user_operation(store, &operation)?
                        .ok_or_else(|| ControlError::NotFound.into())
                })
                .and_then(|receipt| receipt_projection(command, &receipt))
                .map_or_else(error_response, AppResponsePayload::WorkbenchReceipt),
            Err(error) => error.response(),
        }
    }
}

fn branch_execution_allowed(
    branch: &peritus_product_runner::control::ConversationBranch,
    intent: &peritus_product_runner::control::ControlIntent,
    mode: peritus_app_protocol::ProductInteractionMode,
) -> bool {
    use peritus_product_runner::control::{ControlIntent, ConversationBranchMode};

    match (branch.mode(), branch.goal_revision(), intent) {
        (
            ConversationBranchMode::ReadOnlyCurrentWorkspace,
            0,
            ControlIntent::StartExecution { .. },
        ) => mode == peritus_app_protocol::ProductInteractionMode::Chat,
        (
            ConversationBranchMode::ReadOnlyCurrentWorkspace,
            goal_revision,
            ControlIntent::StartGoal { objective, criteria, .. },
        ) => {
            goal_revision != 0
                && mode == peritus_app_protocol::ProductInteractionMode::Chat
                && branch.objective() == Some(objective.as_str())
                && branch.criteria() == criteria
        }
        (
            ConversationBranchMode::IsolatedWritableWorkspace,
            goal_revision,
            ControlIntent::StartGoal { objective, criteria, .. },
        ) => {
            goal_revision != 0
                && branch.objective() == Some(objective.as_str())
                && branch.criteria() == criteria
        }
        _ => false,
    }
}
