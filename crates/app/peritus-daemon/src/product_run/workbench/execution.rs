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
        let admission = self.with_control_conversation(operation.conversation(), |store| {
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
                .with_control_conversation(operation.conversation(), |store| {
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

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_app_protocol::ProductInteractionMode;
    use peritus_product_runner::control::{
        ControlIntent, ControlText, ConversationBranch, ConversationBranchMode, ConversationId,
        GoalCriterion, GoalCriterionKind, OperationId,
    };
    use peritus_types::WorkspaceId;

    fn branch(goal_revision: u64) -> ConversationBranch {
        ConversationBranch::new(
            OperationId::new([1; 16]).expect("operation"),
            ConversationId::new([2; 16]).expect("source"),
            WorkspaceId::new([3; 16]).expect("workspace"),
            1,
            OperationId::new([4; 16]).expect("checkpoint"),
            1,
            1,
            goal_revision,
            ConversationId::new([5; 16]).expect("child"),
            WorkspaceId::new([3; 16]).expect("workspace"),
            ConversationBranchMode::ReadOnlyCurrentWorkspace,
            "Child".to_owned(),
            Some("Objective".to_owned()),
            if goal_revision == 0 {
                Vec::new()
            } else {
                vec![
                    GoalCriterion::new(
                        GoalCriterionKind::RunnerAcceptance,
                        "Acceptance".to_owned(),
                        true,
                    )
                    .expect("criterion"),
                ]
            },
        )
        .expect("branch")
    }

    fn start_goal() -> ControlIntent {
        ControlIntent::StartGoal {
            run: [6; 16],
            settings_digest: [7; 32],
            objective: ControlText::new("Objective".to_owned()).expect("objective"),
            criteria: vec![
                GoalCriterion::new(
                    GoalCriterionKind::RunnerAcceptance,
                    "Acceptance".to_owned(),
                    true,
                )
                .expect("criterion"),
            ],
            now_unix_millis: 1,
        }
    }

    #[test]
    fn read_only_branch_execution_is_exactly_chat_and_governance_shaped() {
        let execution = ControlIntent::StartExecution { run: [8; 16], settings_digest: [9; 32] };
        let ungoverned = branch(0);
        assert!(branch_execution_allowed(&ungoverned, &execution, ProductInteractionMode::Chat));
        assert!(!branch_execution_allowed(&ungoverned, &execution, ProductInteractionMode::Build));

        let governed = branch(1);
        let exact_goal = start_goal();
        assert!(branch_execution_allowed(&governed, &exact_goal, ProductInteractionMode::Chat));
        assert!(!branch_execution_allowed(&governed, &exact_goal, ProductInteractionMode::Build));
        assert!(!branch_execution_allowed(&governed, &execution, ProductInteractionMode::Chat));
        let mut wrong_goal = start_goal();
        if let ControlIntent::StartGoal { objective, .. } = &mut wrong_goal {
            *objective = ControlText::new("Different objective".to_owned()).unwrap();
        }
        assert!(!branch_execution_allowed(&governed, &wrong_goal, ProductInteractionMode::Chat));
    }
}
