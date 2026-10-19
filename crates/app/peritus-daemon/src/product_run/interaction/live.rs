//! Live D0 input, provider selection, admission and typed activity adapter.

use super::{
    InteractionOptions, ProductRunService, ProductRunServiceError, narration, tool_activity,
};
use peritus_agent::DeveloperInput;

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
    fn requests_ready_design(&self) -> bool {
        let Ok(records) = self.service.inner.records.read() else { return false };
        let Some(run) = records.get(&self.run_id) else { return false };
        self.service
            .with_controls(false, |store| {
                let record = store
                    .load(run.interaction.workbench.conversation())?
                    .ok_or(peritus_product_runner::control::ControlError::NotFound)?;
                let capture = store.capture_execution(&run.interaction.workbench)?;
                // The UI action may still be pending its first model request. Use the same
                // dependency/hold-aware selection as admission; do not incorporate it here.
                Ok(capture
                    .inputs()
                    .included()
                    .last()
                    .and_then(|selected| record.inputs().latest(selected.id()))
                    .is_some_and(|input| {
                        input.text().trim()
                            == peritus_product_runner::GUIDED_DESIGN_IMPLEMENT_REQUEST
                    }))
            })
            .unwrap_or(false)
    }
    fn uses_explicit_media(&self) -> bool {
        // An unavailable control binding must not permit a fallback to ambient file discovery.
        self.service.governed_run(self.run_id).unwrap_or(true)
    }
    fn has_selected_file_attachments(&self) -> bool {
        let Ok(records) = self.service.inner.records.read() else { return false };
        let Some(record) = records.get(&self.run_id) else { return false };
        let start = &record.interaction.workbench;
        self.service
            .with_controls(false, |store| {
                let control = store
                    .load(start.conversation())?
                    .ok_or(peritus_product_runner::control::ControlError::NotFound)?;
                Ok(control
                    .files()
                    .entries()
                    .iter()
                    .any(peritus_product_runner::control::FileSelection::selected))
            })
            .unwrap_or(false)
    }
    fn read_attachment_range(
        &self,
        request: peritus_product_runner::AttachmentReadRequest,
    ) -> Result<peritus_product_runner::AttachmentReadResponse, String> {
        let records =
            self.service.inner.records.read().map_err(|_| "conversation unavailable".to_owned())?;
        let record =
            records.get(&self.run_id).ok_or_else(|| "conversation unavailable".to_owned())?;
        let start = &record.interaction.workbench;
        self.service
            .with_controls(false, |store| store.read_file_page(start, request))
            .map_err(|_| "selected immutable attachment is unavailable or changed".to_owned())
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
            let start = &record.interaction.workbench;
            self.service.with_controls(false, |store| {
                store.capture_execution(start)?;
                let record = store.load(start.conversation())?.ok_or(peritus_product_runner::control::ControlError::NotFound)?;
                let text = record.inputs().incorporated_conversation()?;
                Ok(if text.is_empty() {
                    "Current user instructions are supplied by the host at the request admission boundary.".to_owned()
                } else {
                    text
                })
            }).map_err(ProductRunServiceError::from)
        })();
        result.unwrap_or_else(|_| {
            "Governing conversation unavailable; execution must stop.".to_owned()
        })
    }
    fn reference_authority_context(&self) -> String {
        let result = (|| {
            let records = self
                .service
                .inner
                .records
                .read()
                .map_err(|_| ProductRunServiceError::Unavailable)?;
            let record = records.get(&self.run_id).ok_or(ProductRunServiceError::NotFound)?;
            let start = &record.interaction.workbench;
            self.service
                .with_controls(false, |store| store.capture_execution(start))
                .map(|capture| capture.reference_authority_context().to_owned())
                .map_err(ProductRunServiceError::from)
        })();
        result.unwrap_or_default()
    }
    fn take_module_command(&self) -> Result<Option<String>, String> {
        self.admit_module_command().map_err(|e| e.to_string())
    }
    fn present_host_reply(&self, text: &str) -> Result<(), String> {
        self.update(|options, _| {
            let mut remaining = text;
            while !remaining.is_empty() {
                let end =
                    remaining.floor_char_boundary(peritus_app_protocol::MAX_PRODUCT_ACTIVITY_BYTES);
                options.append(
                    ProductActivityKind::Assistant,
                    &remaining[..end],
                    "Local module result",
                )?;
                remaining = &remaining[end..];
            }
            Ok(())
        })
        .map_err(|e| e.to_string())
    }
    fn incorporated_revision(&self) -> u64 {
        self.service
            .inner
            .records
            .read()
            .ok()
            .and_then(|records| {
                records.get(&self.run_id).map(|record| record.interaction.incorporated)
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
            .map_or_else(|_| vec![PathBuf::new()], |record| record.reviews().protected_paths())
    }
    fn effective_permissions(&self) -> HostPermissions {
        // Tool boundaries must not retain ambient authority when the durable policy cannot be
        // read or its run/workspace binding is unavailable.
        self.service.effective_permissions(self.run_id).unwrap_or_else(|_| HostPermissions::none())
    }
    fn permits_pipeline_handoff(&self) -> bool {
        self.review_record().is_ok_and(|record| {
            record.reviews().pending_pipeline_permission(record.inputs()).unwrap_or(true)
        })
    }
    fn checkpoint_before_workspace_mutation(
        &self,
        relative_path: &std::path::Path,
        kind: WorkspaceMutationKind,
    ) -> Result<(), String> {
        let start = self
            .workbench_start_record()
            .map_err(|error| format!("automatic workspace checkpoint is unavailable: {error}. Peritus did not change the workspace"))?;
        self.service
            .capture_automatic_checkpoint(&start, self.run_id, relative_path, kind)
            .map_err(|error| format!("automatic workspace checkpoint could not be durably captured: {error}. Peritus did not change the workspace"))
    }
    fn seal_workspace_mutation_checkpoint(
        &self,
        relative_path: &std::path::Path,
        kind: WorkspaceMutationKind,
        owned_postchange: peritus_product_runner::control::CheckpointFileVersion,
    ) -> Result<(), String> {
        let start = self
            .workbench_start_record()
            .map_err(|error| format!("automatic workspace checkpoint is unavailable: {error}. Peritus stopped before accepting another workspace mutation"))?;
        self.service
            .seal_automatic_checkpoint(&start, self.run_id, relative_path, kind, owned_postchange)
            .map_err(|error| format!("automatic workspace checkpoint could not be durably sealed: {error}. Peritus stopped because the completed mutation could not be recorded durably"))
    }

    fn interaction(&self) -> Option<&dyn DeveloperInteraction> {
        Some(self)
    }
}

impl DeveloperInteraction for LiveConversation {
    fn record_observation_images(
        &self,
        call: &peritus_model_protocol::CompletedToolCall,
        images: &[peritus_model_protocol::MediaInput],
    ) -> Result<(), DeveloperLoopError> {
        let observations = images
            .iter()
            .map(|image| {
                crate::product_control::ToolImageObservation::from_tool(call, image).map_err(
                    |error| {
                        port_error(
                            "record host image provenance",
                            crate::product_control::ControlStoreError::from(error).into(),
                        )
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.update(|options, _| {
            for observation in observations {
                if !options.observed_images.contains(&observation) {
                    options.observed_images.push(observation);
                }
            }
            Ok(())
        })
    }
    fn allows_semantic_compaction(&self) -> bool {
        false
    }
    fn provider(
        &self,
        role: DeveloperModelRole,
    ) -> Result<Option<Arc<dyn peritus_provider_core::ModelProvider>>, DeveloperLoopError> {
        let records = self.service.inner.records.read().map_err(|_| {
            port_internal("select the run provider", "the product-run record lock was poisoned")
        })?;
        let record = records.get(&self.run_id).ok_or_else(|| {
            port_internal("select the run provider", "the product-run record was not found")
        })?;
        let options = &record.interaction;
        if options.persistence_failed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(port_internal(
                "select the run provider",
                options
                    .persistence_failure()
                    .as_deref()
                    .unwrap_or("the previous persistence operation failed"),
            ));
        }
        let providers = record.request.providers();
        let (profile, choice) = match role {
            DeveloperModelRole::Writer => (providers.writer(), options.models.writer()),
            DeveloperModelRole::Reviewer => (providers.reviewer(), options.models.reviewer()),
            DeveloperModelRole::Fixer => (providers.fixer(), options.models.fixer()),
        };
        self.service
            .select_provider(profile, choice)
            .map(Some)
            .map_err(|error| port_error("select the run provider", error))
    }

    fn input(&self) -> Result<DeveloperInput, DeveloperLoopError> {
        // Admission holds the write lock until persistence succeeds. A model cannot observe an
        // input revision halfway through its durable receive transaction.
        let records = self.service.inner.records.read().map_err(|_| {
            port_internal(
                "read the governing conversation",
                "the product-run record lock was poisoned",
            )
        })?;
        let record = records.get(&self.run_id).ok_or_else(|| {
            port_internal("read the governing conversation", "the product-run record was not found")
        })?;
        if record.interaction.persistence_failed.load(std::sync::atomic::Ordering::Acquire) {
            let detail = record
                .interaction
                .persistence_failure()
                .unwrap_or_else(|| "the previous persistence operation failed".to_owned());
            return Err(port_internal("read the governing conversation", &detail));
        }
        let start = record.interaction.workbench.clone();
        let provider = record.request.providers().writer();
        let model = record.interaction.models.writer().clone();
        drop(records);
        self.service
            .refresh_request_files(&start, provider, &model)
            .map_err(|error| port_error("capture selected source snapshot", error))?;
        let records = self.service.inner.records.read().map_err(|_| {
            port_internal(
                "read the governing conversation",
                "the product-run record lock was poisoned",
            )
        })?;
        let record = records.get(&self.run_id).ok_or_else(|| {
            port_internal("read the governing conversation", "the product-run record was not found")
        })?;
        self.service
            .record_input(record)
            .map_err(|error| port_error("read the governing conversation", error))
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
        let start = self.workbench_start()?;
        let admission = self
            .service
            .with_controls(false, |store| {
                store.complete_goal_request(&start, goal_role(role), request_id, usage)
            })
            .map_err(|error| {
                port_error("commit model request usage to durable control state", error.into())
            })?;
        self.update(|_, progress| {
            progress.complete_provider_request(usage);
            Ok(())
        })?;
        Ok(control_flow(admission))
    }

    fn admit_tool(
        &self,
        role: DeveloperModelRole,
        invocation: &str,
        sequence: u32,
        input_revision: u64,
        effect: DeveloperToolEffect,
    ) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        let start = self.workbench_start()?;
        let admission = self
            .service
            .with_controls(false, |store| {
                if store.capture_execution(&start)?.inputs().generation() != input_revision {
                    return Ok(None);
                }
                store
                    .reserve_goal_tool(
                        &start,
                        goal_role(role),
                        invocation,
                        sequence,
                        effect == DeveloperToolEffect::MutationCapable,
                    )
                    .map(Some)
            })
            .map_err(|error| {
                port_error("reserve a tool call in durable control state", error.into())
            })?;
        Ok(admission.map_or(DeveloperControlFlow::Yield, control_flow))
    }

    fn complete_tool(
        &self,
        role: DeveloperModelRole,
        invocation: &str,
        sequence: u32,
    ) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        let start = self.workbench_start()?;
        let admission = self
            .service
            .with_controls(false, |store| {
                store.complete_goal_tool(&start, goal_role(role), invocation, sequence)
            })
            .map_err(|error| {
                port_error("commit a tool result to durable control state", error.into())
            })?;
        Ok(control_flow(admission))
    }

    fn observe(&self, activity: DeveloperActivity<'_>) -> Result<(), DeveloperLoopError> {
        self.update(|options, progress| {
            match activity {
            DeveloperActivity::Text(bytes) => {
                progress.mark_event("provider text received");
                options.text(bytes)
            }
            DeveloperActivity::ReasoningSummary(bytes) => {
                progress.mark_event("provider summary received");
                options.summary(bytes)
            }
            DeveloperActivity::ModelStarted { model, reasoning } => {
                progress.begin_provider_request();
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
            DeveloperActivity::ModelWaiting { elapsed_seconds, progress: observed } => {
                progress.mark_event(match observed {
                    peritus_agent::DeveloperProviderProgress::AwaitingResponse => "provider still waiting",
                    peritus_agent::DeveloperProviderProgress::Accepted => "provider runtime accepted request",
                    peritus_agent::DeveloperProviderProgress::Reasoning => "provider reasoning",
                    peritus_agent::DeveloperProviderProgress::Generating { .. } => "provider response arriving",
                });
                narration::waiting(options, elapsed_seconds, observed)
            }
            DeveloperActivity::ResponseHealed => {
                progress.mark_event("provider response repaired");
                options.append(
                    ProductActivityKind::Status,
                    "Repaired model JSON formatting",
                    "Original and repaired values are retained in the private trace. Tool validation and permissions still apply.",
                )
            }
            DeveloperActivity::ReviewRetry { next_attempt, reason } => {
                progress.mark_event("review retry scheduled");
                narration::review_retry(options, next_attempt, reason)
            }
            DeveloperActivity::ToolStarted { name, arguments } => {
                progress.begin_tool(name);
                tool_activity::started(options, name, arguments)
            }
            DeveloperActivity::ToolFinished { name, output, is_error } => {
                progress.mark_event(&format!("tool finished: {name}"));
                tool_activity::finished(options, name, output, is_error)
            }
            DeveloperActivity::ToolSkipped { name } => {
                progress.mark_event(&format!("tool skipped: {name}"));
                options.append(
                    ProductActivityKind::Status,
                    &format!("Skipped {name}: control returned before execution"),
                    "",
                )
            }
        }
        })
    }
}

mod modules;
mod request;
mod review;

const fn goal_role(role: DeveloperModelRole) -> peritus_product_runner::control::GoalRole {
    match role {
        DeveloperModelRole::Writer => peritus_product_runner::control::GoalRole::Writer,
        DeveloperModelRole::Reviewer => peritus_product_runner::control::GoalRole::Reviewer,
        DeveloperModelRole::Fixer => peritus_product_runner::control::GoalRole::Fixer,
    }
}

const fn control_flow(
    admission: peritus_product_runner::control::GoalAdmission,
) -> DeveloperControlFlow {
    match admission {
        peritus_product_runner::control::GoalAdmission::Accepted => DeveloperControlFlow::Continue,
        peritus_product_runner::control::GoalAdmission::Paused
        | peritus_product_runner::control::GoalAdmission::Inactive => DeveloperControlFlow::Stop,
    }
}

fn port_error(operation: &'static str, error: ProductRunServiceError) -> DeveloperLoopError {
    DeveloperLoopError::Trace(format!("{operation}: {error}"))
}

fn port_internal(operation: &'static str, detail: &str) -> DeveloperLoopError {
    DeveloperLoopError::Trace(format!("{operation}: {detail}"))
}
