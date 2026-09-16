//! Live D0 input, provider selection, admission and typed activity adapter.

#[cfg(not(verus_only))]
use super::persist_record;
use super::{
    InteractionOptions, ProductRunService, ProductRunServiceError, narration, tool_activity,
};
use peritus_agent::DeveloperInput;
#[cfg(not(verus_only))]
use peritus_agent::{
    DeveloperActivity, DeveloperControlFlow, DeveloperInteraction, DeveloperLoopError,
    DeveloperModelRole, DeveloperRequestAdmission, DeveloperToolEffect,
};
use peritus_app_protocol::{ProductActivityKind, ProductInteractionMode};
use peritus_product_runner::{ConversationView, WorkspaceMutationKind, control::HostPermissions};
use peritus_types::RunId;
use std::{path::PathBuf, sync::Arc};

pub(super) struct LiveConversation {
    pub(super) service: ProductRunService,
    pub(super) run_id: RunId,
}
impl ConversationView for LiveConversation {
    fn uses_explicit_media(&self) -> bool {
        // An unavailable control binding must not permit a fallback to ambient file discovery.
        self.service.governed_run(self.run_id).unwrap_or(true)
    }
    fn stable_request_context(&self) -> String {
        let result = (|| {
            let records = self
                .service
                .inner
                .records
                .read()
                .map_err(|_| ProductRunServiceError::Unavailable)?;
            let record = records.get(&self.run_id).ok_or(ProductRunServiceError::NotFound)?;
            let Some(start) =
                record.interaction.as_ref().and_then(|options| options.workbench.as_ref())
            else {
                return Ok(record.conversation.render());
            };
            self.service.with_controls(false, |store| {
                store.capture_execution(start)?;
                let record = store.load(start.conversation())?.ok_or(peritus_product_runner::control::ControlError::NotFound)?;
                let text = record.inputs().incorporated_conversation()?;
                Ok(if text.is_empty() { "Current user instructions are supplied by the host at the request admission boundary.".to_owned() } else { text })
            }).map_err(ProductRunServiceError::from)
        })();
        result.unwrap_or_else(|_| {
            "Governing conversation unavailable; execution must stop.".to_owned()
        })
    }
    fn incorporated_revision(&self) -> u64 {
        self.service
            .inner
            .records
            .read()
            .ok()
            .and_then(|records| {
                records
                    .get(&self.run_id)
                    .and_then(|record| record.interaction.as_ref())
                    .map(|options| options.incorporated)
            })
            .unwrap_or(0)
    }
    fn revision(&self) -> u64 {
        self.service
            .inner
            .records
            .read()
            .ok()
            .and_then(|records| {
                records
                    .get(&self.run_id)
                    .and_then(|record| self.service.record_input_revision(record).ok())
            })
            .unwrap_or(u64::MAX)
    }
    fn render(&self) -> String {
        self.service
            .inner
            .records
            .read()
            .ok()
            .and_then(|records| {
                records.get(&self.run_id).and_then(|record| self.service.record_input(record).ok())
            })
            .map_or_else(
                || "Governing conversation unavailable; execution must stop.".to_owned(),
                |input| input.conversation,
            )
    }
    fn protected_paths(&self) -> Vec<PathBuf> {
        self.review_record()
            // An unavailable narrowing record must prevent mutation while retaining read-only
            // diagnosis. Empty relative path means the complete workspace mutation surface.
            .map_or_else(
                |_| vec![PathBuf::new()],
                |record| record.map_or_else(Vec::new, |record| record.reviews().protected_paths()),
            )
    }
    fn effective_permissions(&self) -> HostPermissions {
        // Tool boundaries must not retain ambient authority when the durable policy cannot be
        // read or its run/workspace binding is unavailable.
        self.service.effective_permissions(self.run_id).unwrap_or_else(|_| HostPermissions::none())
    }
    fn permits_pipeline_handoff(&self) -> bool {
        self.review_record().is_ok_and(|record| {
            record.is_none_or(|record| {
                record.reviews().pending_pipeline_permission(record.inputs()).unwrap_or(true)
            })
        })
    }
    fn checkpoint_before_workspace_mutation(
        &self,
        relative_path: &std::path::Path,
        kind: WorkspaceMutationKind,
    ) -> Result<(), String> {
        let start = self
            .workbench_start_record()
            .map_err(|_| "automatic workspace checkpoint is unavailable".to_owned())?;
        let Some(start) = start else { return Ok(()) };
        self.service
            .capture_automatic_checkpoint(&start, self.run_id, relative_path, kind)
            .map_err(|_| "automatic workspace checkpoint could not be durably captured".to_owned())
    }
    fn seal_workspace_mutation_checkpoint(
        &self,
        relative_path: &std::path::Path,
        kind: WorkspaceMutationKind,
        owned_postchange: peritus_product_runner::control::CheckpointFileVersion,
    ) -> Result<(), String> {
        let start = self
            .workbench_start_record()
            .map_err(|_| "automatic workspace checkpoint is unavailable".to_owned())?;
        let Some(start) = start else { return Ok(()) };
        self.service
            .seal_automatic_checkpoint(&start, self.run_id, relative_path, kind, owned_postchange)
            .map_err(|_| "automatic workspace checkpoint could not be durably sealed".to_owned())
    }
    #[cfg(not(verus_only))]
    fn interaction(&self) -> Option<&dyn DeveloperInteraction> {
        Some(self)
    }
}
#[cfg(not(verus_only))]
impl DeveloperInteraction for LiveConversation {
    fn allows_semantic_compaction(&self) -> bool {
        self.service.governed_run(self.run_id).is_ok_and(|governed| !governed)
    }
    fn provider(
        &self,
        role: DeveloperModelRole,
    ) -> Result<Option<Arc<dyn peritus_provider_core::ModelProvider>>, DeveloperLoopError> {
        let records = self.service.inner.records.read().map_err(|_| port_error())?;
        let record = records.get(&self.run_id).ok_or_else(port_error)?;
        let options = record.interaction.as_ref().ok_or_else(port_error)?;
        if options.persistence_failed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(port_error());
        }
        let providers = record.request.providers();
        let (profile, choice) = match role {
            DeveloperModelRole::Writer => (providers.writer(), options.models.writer()),
            DeveloperModelRole::Reviewer => (providers.reviewer(), options.models.reviewer()),
            DeveloperModelRole::Fixer => (providers.fixer(), options.models.fixer()),
        };
        self.service.select_provider(profile, choice).map(Some).map_err(|_| port_error())
    }

    fn input(&self) -> Result<DeveloperInput, DeveloperLoopError> {
        // Admission holds the write lock until persistence succeeds. A model cannot observe an
        // input revision halfway through its durable receive transaction.
        let records = self.service.inner.records.read().map_err(|_| port_error())?;
        let record = records.get(&self.run_id).ok_or_else(port_error)?;
        if record.interaction.as_ref().is_some_and(|options| {
            options.persistence_failed.load(std::sync::atomic::Ordering::Acquire)
        }) {
            return Err(port_error());
        }
        self.service.record_input(record).map_err(|_| port_error())
    }
    fn prepare_request(
        &self,
        revision: u64,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<DeveloperRequestAdmission, DeveloperLoopError> {
        self.prepare_request_for_role(DeveloperModelRole::Writer, revision, request)
    }

    fn prepare_role_request(
        &self,
        role: DeveloperModelRole,
        revision: u64,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<DeveloperRequestAdmission, DeveloperLoopError> {
        self.prepare_request_for_role(role, revision, request)
    }

    fn complete_role_request(
        &self,
        role: DeveloperModelRole,
        request_id: &str,
        usage: peritus_model_protocol::UsageCounters,
    ) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        let Some(start) = self.workbench_start()? else {
            return Ok(DeveloperControlFlow::Continue);
        };
        let admission = self
            .service
            .with_controls(false, |store| {
                store.complete_goal_request(&start, goal_role(role), request_id, usage)
            })
            .map_err(|_| port_error())?;
        Ok(control_flow(admission))
    }

    fn admit_tool(
        &self,
        role: DeveloperModelRole,
        invocation: &str,
        sequence: u32,
        effect: DeveloperToolEffect,
    ) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        let Some(start) = self.workbench_start()? else {
            return Ok(DeveloperControlFlow::Continue);
        };
        let admission = self
            .service
            .with_controls(false, |store| {
                store.reserve_goal_tool(
                    &start,
                    goal_role(role),
                    invocation,
                    sequence,
                    effect == DeveloperToolEffect::MutationCapable,
                )
            })
            .map_err(|_| port_error())?;
        Ok(control_flow(admission))
    }

    fn complete_tool(
        &self,
        role: DeveloperModelRole,
        invocation: &str,
        sequence: u32,
    ) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        let Some(start) = self.workbench_start()? else {
            return Ok(DeveloperControlFlow::Continue);
        };
        let admission = self
            .service
            .with_controls(false, |store| {
                store.complete_goal_tool(&start, goal_role(role), invocation, sequence)
            })
            .map_err(|_| port_error())?;
        Ok(control_flow(admission))
    }

    fn observe(&self, activity: DeveloperActivity<'_>) -> Result<(), DeveloperLoopError> {
        self.update(|options| match activity {
            DeveloperActivity::Text(bytes) => options.text(bytes),
            DeveloperActivity::ModelStarted { model, reasoning } => {
                options.streaming_text = false;
                let effort = match reasoning {
                    peritus_model_protocol::ReasoningPolicy::Disabled => "not requested",
                    peritus_model_protocol::ReasoningPolicy::Adaptive { .. } => "adaptive",
                    peritus_model_protocol::ReasoningPolicy::Effort { effort, .. } => {
                        effort.as_str()
                    }
                };
                options.append(
                    ProductActivityKind::Status,
                    &format!("Requesting model {model} · effort {effort}"),
                    "",
                )
            }
            DeveloperActivity::ModelWaiting { elapsed_seconds } => {
                narration::waiting(options, elapsed_seconds)
            }
            DeveloperActivity::ResponseHealed => options.append(
                ProductActivityKind::Status,
                "Repaired model JSON formatting",
                "Original and repaired values are retained in the private trace. Tool validation and permissions still apply.",
            ),
            DeveloperActivity::ToolStarted { name, arguments } => {
                tool_activity::started(options, name, arguments)
            }
            DeveloperActivity::ToolFinished { name, output, is_error } => {
                tool_activity::finished(options, name, output, is_error)
            }
            DeveloperActivity::ToolSkipped { name } => options.append(
                ProductActivityKind::Status,
                &format!("Skipped {name}: control returned before execution"),
                "",
            ),
        })
    }
}
#[cfg(not(verus_only))]
mod request;
mod review;

#[cfg(not(verus_only))]
const fn goal_role(role: DeveloperModelRole) -> peritus_product_runner::control::GoalRole {
    match role {
        DeveloperModelRole::Writer => peritus_product_runner::control::GoalRole::Writer,
        DeveloperModelRole::Reviewer => peritus_product_runner::control::GoalRole::Reviewer,
        DeveloperModelRole::Fixer => peritus_product_runner::control::GoalRole::Fixer,
    }
}

#[cfg(not(verus_only))]
const fn control_flow(
    admission: peritus_product_runner::control::GoalAdmission,
) -> DeveloperControlFlow {
    match admission {
        peritus_product_runner::control::GoalAdmission::Accepted => DeveloperControlFlow::Continue,
        peritus_product_runner::control::GoalAdmission::Paused
        | peritus_product_runner::control::GoalAdmission::BudgetReached
        | peritus_product_runner::control::GoalAdmission::Inactive => DeveloperControlFlow::Stop,
    }
}
#[cfg(not(verus_only))]
fn port_error() -> DeveloperLoopError {
    DeveloperLoopError::Trace("durable conversation activity unavailable".to_owned())
}
