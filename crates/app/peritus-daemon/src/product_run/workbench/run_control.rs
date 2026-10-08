//! Authenticated adaptation of run dashboard controls to durable conversation authority.

use super::{ProductRunService, error_response};
use crate::product_run::ProductRunServiceError;
use peritus_app_protocol::{
    AppResponsePayload, ControlOperationId, ConversationId, ProductRunControl,
    ProductRunControlAction, RequestId, WorkbenchCommand, WorkbenchGoalPauseMode, WorkbenchIntent,
    WorkbenchQuery,
};
use peritus_product_runner::control::ControlError;
use peritus_types::ActorId;

impl ProductRunService {
    pub(crate) async fn control_authenticated(
        &self,
        actor: ActorId,
        request: RequestId,
        control: ProductRunControl,
    ) -> AppResponsePayload {
        let service = self.clone();
        Self::await_blocking_future("apply authenticated product-run control", move || async move {
            service.control_authenticated_owned(actor, request, control).await
        })
        .await
        .unwrap_or_else(ProductRunServiceError::response)
    }

    async fn control_authenticated_owned(
        &self,
        actor: ActorId,
        request: RequestId,
        control: ProductRunControl,
    ) -> AppResponsePayload {
        if control.action() == ProductRunControlAction::Cancel {
            match self.signal_authenticated_user_cancellation(actor, control.run_id()) {
                Ok(true) => {
                    return self
                        .cancel(control.run_id())
                        .and_then(|snapshot| self.project(snapshot))
                        .unwrap_or_else(ProductRunServiceError::response);
                }
                Ok(false) => {}
                Err(error) => return error.response(),
            }
        }
        let binding = self
            .inner
            .records
            .read()
            .map_err(|_| ProductRunServiceError::Unavailable)
            .and_then(|records| {
                records
                    .get(&control.run_id())
                    .map(|record| record.interaction.workbench.clone())
                    .ok_or(ProductRunServiceError::NotFound)
            });
        let binding = match binding {
            Ok(binding) => binding,
            Err(error) => return error.response(),
        };
        if binding.actor_bytes() != actor.as_bytes() {
            return error_response(ControlError::ScopeMismatch.into());
        }
        if control.action() == ProductRunControlAction::Cancel {
            if let Err(error) = self.ensure_control_legal(control.run_id(), control.action()) {
                return error.response();
            }
            if matches!(
                binding.intent(),
                peritus_product_runner::control::ControlIntent::StartExecution { .. }
            ) {
                // Ungoverned run cancellation has no second journal CAS. The lifecycle owner marks
                // the run cancelled before waking its control waiters, so a released callback can
                // only observe the already-established cancellation boundary.
                if !self.signal_user_cancellation(control.run_id()) {
                    return ProductRunServiceError::NotFound.response();
                }
                return self
                    .cancel(control.run_id())
                    .and_then(|snapshot| self.project(snapshot))
                    .unwrap_or_else(ProductRunServiceError::response);
            }
        }
        self.control_authenticated_governed(actor, request, control, binding).await
    }

    async fn control_authenticated_governed(
        &self,
        actor: ActorId,
        request: RequestId,
        control: ProductRunControl,
        binding: peritus_product_runner::control::ControlOperation,
    ) -> AppResponsePayload {
        let Ok(conversation) = ConversationId::new(*binding.conversation().as_bytes()) else {
            return error_response(ControlError::InvalidInput.into());
        };
        let Ok(workspace) = peritus_types::WorkspaceId::new(*binding.workspace_bytes()) else {
            return error_response(ControlError::InvalidInput.into());
        };
        let query = WorkbenchQuery::new(conversation, workspace);
        let state = match self.workbench_execution(actor, query) {
            AppResponsePayload::WorkbenchExecution(state)
                if state.run() == Some(control.run_id()) =>
            {
                state
            }
            AppResponsePayload::WorkbenchExecution(_) => {
                return error_response(ControlError::ScopeMismatch.into());
            }
            error => return error,
        };
        if state.has_goal()
            && matches!(
                control.action(),
                ProductRunControlAction::Cancel | ProductRunControlAction::Retry
            )
        {
            return self.control_goal_run(actor, request, query, control).await;
        }
        if let Err(error) = self.ensure_control_legal(control.run_id(), control.action()) {
            return error.response();
        }
        let required: &[peritus_product_runner::control::PermissionCapability] =
            match control.action() {
                ProductRunControlAction::Commit | ProductRunControlAction::Discard => {
                    &[peritus_product_runner::control::PermissionCapability::Write]
                }
                ProductRunControlAction::Export => {
                    &[peritus_product_runner::control::PermissionCapability::Read]
                }
                _ => &[],
            };
        if let Err(error) = self.require_workspace_permissions(actor, query, required) {
            return error_response(error);
        }
        let result = match control.action() {
            ProductRunControlAction::Cancel => {
                if !self.signal_user_cancellation(control.run_id()) {
                    Err(ProductRunServiceError::NotFound)
                } else {
                    self.cancel(control.run_id())
                }
            }
            ProductRunControlAction::Retry => self.retry(control.run_id()).await,
            ProductRunControlAction::Acknowledge => {
                self.acknowledge_command_outcome(control.run_id())
            }
            action => self.control_deliverable(control.run_id(), action),
        };
        result
            .and_then(|snapshot| self.project(snapshot))
            .unwrap_or_else(ProductRunServiceError::response)
    }

    async fn control_goal_run(
        &self,
        actor: ActorId,
        request: RequestId,
        query: WorkbenchQuery,
        control: ProductRunControl,
    ) -> AppResponsePayload {
        let goal = match self.workbench_goal(actor, query) {
            AppResponsePayload::WorkbenchGoal(goal) => goal,
            error => return error,
        };
        let mut bytes = b"peritus-dashboard-goal-control-v1".to_vec();
        bytes.extend_from_slice(request.as_bytes());
        bytes.extend_from_slice(actor.as_bytes());
        let digest = peritus_codec::sha256(&bytes);
        let mut id = [0; 16];
        id.copy_from_slice(&digest.as_bytes()[..16]);
        let Ok(operation) = ControlOperationId::new(id) else {
            return error_response(ControlError::InvalidInput.into());
        };
        let intent = if control.action() == ProductRunControlAction::Cancel {
            WorkbenchIntent::PauseGoal { goal: goal.goal(), mode: WorkbenchGoalPauseMode::Now }
        } else {
            WorkbenchIntent::ResumeGoal { goal: goal.goal() }
        };
        let command = WorkbenchCommand::new(operation, query, goal.aggregate_revision(), intent);
        match self.workbench_command(actor, &command).await {
            AppResponsePayload::WorkbenchReceipt(_) => self
                .query_interaction(peritus_app_protocol::ProductInteractionQuery::new(
                    control.run_id(),
                ))
                .map_or_else(ProductRunServiceError::response, AppResponsePayload::Interaction),
            error => error,
        }
    }
}
