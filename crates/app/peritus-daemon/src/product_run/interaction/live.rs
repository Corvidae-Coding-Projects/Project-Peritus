//! Live D0 input, provider selection, admission and typed activity adapter.

use super::{
    InteractionOptions, ProductRunService, ProductRunServiceError, narration, tool_activity,
};
use crate::product_control::RequestSourceSnapshot;
use peritus_agent::DeveloperInput;
#[cfg(not(verus_only))]
use peritus_agent::{
    DeveloperActivity, DeveloperControlFlow, DeveloperInteraction, DeveloperLoopError,
    DeveloperModelRole, DeveloperProviderSelection, DeveloperRequestAdmission,
    DeveloperToolEffect,
};
use peritus_app_protocol::{ProductActivityKind, ProductModelChoice};
use peritus_types::{ProviderProfileId, RunId, Sha256Digest};
use std::{pin::Pin, sync::Arc};

pub(super) struct LiveConversation {
    pub(super) service: ProductRunService,
    pub(super) run_id: RunId,
    pub(super) attempt_cancelled: Arc<std::sync::atomic::AtomicBool>,
    pub(super) request_sources: std::sync::Mutex<Option<Arc<RequestSourceSnapshot>>>,
}
impl LiveConversation {
    pub(super) fn request_source_snapshot(
        &self,
    ) -> Result<Arc<RequestSourceSnapshot>, ProductRunServiceError> {
        {
            let cached = self.request_sources.lock().map_err(|_| {
                ProductRunServiceError::internal(
                    "read admitted conversation sources",
                    "request-source snapshot lock was poisoned",
                )
            })?;
            if let Some(snapshot) = cached.as_ref() {
                return Ok(Arc::clone(snapshot));
            }
        }
        self.current_request_source_snapshot()
    }

    /// Reconciles the cached provider snapshot with the exact current durable input generation.
    /// Source tools use `request_source_snapshot` and therefore retain their admitted view;
    /// obligation boundaries call this method before adopting a conversation revision.
    pub(super) fn current_request_source_snapshot(
        &self,
    ) -> Result<Arc<RequestSourceSnapshot>, ProductRunServiceError> {
        let record = self.attempt_record()?;
        let captured = self.service.record_capture(&record)?;
        let snapshot = self
            .service
            .with_control_conversation(captured.conversation(), |store| {
                store.request_source_snapshot(&captured)
            })
            .map_err(ProductRunServiceError::from)?;
        let candidate = Arc::new(snapshot);
        let mut cached = self.request_sources.lock().map_err(|_| {
            ProductRunServiceError::internal(
                "read admitted conversation sources",
                "request-source snapshot lock was poisoned",
            )
        })?;
        if let Some(existing) = cached.as_ref()
            && existing.generation() == candidate.generation()
        {
            if existing.authority_binding() != candidate.authority_binding()
                || existing.catalog_binding() != candidate.catalog_binding()
            {
                return Err(ProductRunServiceError::internal(
                    "reconcile authoritative conversation sources",
                    "one durable input generation produced conflicting source bindings",
                ));
            }
            return Ok(Arc::clone(existing));
        }
        *cached = Some(Arc::clone(&candidate));
        Ok(candidate)
    }

    pub(super) fn bind_request_source_snapshot(
        &self,
        generation: u64,
        snapshot: RequestSourceSnapshot,
    ) -> Result<(), ProductRunServiceError> {
        if snapshot.generation() != generation {
            return Err(ProductRunServiceError::internal(
                "bind admitted conversation sources",
                "request-source snapshot generation differs from provider admission",
            ));
        }
        let mut cached = self.request_sources.lock().map_err(|_| {
            ProductRunServiceError::internal(
                "bind admitted conversation sources",
                "request-source snapshot lock was poisoned",
            )
        })?;
        *cached = Some(Arc::new(snapshot));
        Ok(())
    }

    fn release_request_source_snapshot(&self) -> Result<(), ProductRunServiceError> {
        let mut cached = self.request_sources.lock().map_err(|_| {
            ProductRunServiceError::internal(
                "release admitted conversation sources",
                "request-source snapshot lock was poisoned",
            )
        })?;
        *cached = None;
        Ok(())
    }
}

#[cfg(not(verus_only))]
pub(super) fn provider_selection_provenance(
    profile: ProviderProfileId,
    choice: &ProductModelChoice,
) -> Sha256Digest {
    let mut bytes = b"peritus-product-provider-selection/v1\0".to_vec();
    bytes.extend_from_slice(profile.as_bytes());
    bytes.extend_from_slice(
        &u64::try_from(choice.id().len()).unwrap_or(u64::MAX).to_le_bytes(),
    );
    bytes.extend_from_slice(choice.id().as_bytes());
    bytes.push(u8::from(choice.manual()));
    bytes.extend_from_slice(&choice.effort().tag().to_le_bytes());
    peritus_codec::sha256(&bytes)
}

#[cfg(not(verus_only))]
impl DeveloperInteraction for LiveConversation {
    fn observe_waiting(
        &self,
        elapsed_seconds: u64,
    ) -> Pin<Box<dyn Future<Output = Result<(), DeveloperLoopError>> + Send + '_>> {
        let service = self.service.clone();
        // Capture only cheap identity fields. Contention defers this projection; it never
        // monopolizes the provider poll or admits work against an invented owner.
        let notice = waiting::capture(&service, self.run_id, elapsed_seconds);
        Box::pin(async move {
            let notice = notice?;
            tokio::task::spawn_blocking(move || {
                let result = waiting::retain(&service, &notice);
                if let Err(error) = &result {
                    crate::diagnostic::report(&format!(
                        "peritusd: waiting status outbox delivery is deferred: {error}; the provider request remains owned",
                    ));
                }
                result
            })
                .await
                .map_err(|error| DeveloperLoopError::Trace(format!("retain waiting notice: {error}")))?
                .map_err(|error| port_error("retain waiting notice", error))
        })
    }

    fn waiting_observation_failed(&self, error: &DeveloperLoopError) {
        crate::diagnostic::report(&format!(
            "peritusd: waiting status delivery for {:?} is deferred; the accepted provider request remains owned: {error}",
            self.run_id,
        ));
    }

    fn allows_semantic_compaction(&self) -> bool {
        false
    }
    fn provider(
        &self,
        role: DeveloperModelRole,
    ) -> Result<Option<Arc<dyn peritus_provider_core::ModelProvider>>, DeveloperLoopError> {
        self.provider_selection(role)
            .map(|selection| selection.map(DeveloperProviderSelection::into_provider))
    }

    fn provider_selection(
        &self,
        role: DeveloperModelRole,
    ) -> Result<Option<DeveloperProviderSelection>, DeveloperLoopError> {
        let record = self
            .attempt_record()
            .map_err(|error| port_error("select the run provider", error))?;
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
        let provenance = provider_selection_provenance(profile, choice);
        self.service
            .select_provider(profile, choice)
            .map(|provider| Some(DeveloperProviderSelection::new(provider, provenance)))
            .map_err(|error| port_error("select the run provider", error))
    }

    fn input(&self) -> Result<DeveloperInput, DeveloperLoopError> {
        let record = self
            .attempt_record()
            .map_err(|error| port_error("read the governing conversation", error))?;
        if record.interaction.persistence_failed.load(std::sync::atomic::Ordering::Acquire) {
            let detail = record
                .interaction
                .persistence_failure()
                .unwrap_or_else(|| "the previous persistence operation failed".to_owned());
            return Err(port_internal("read the governing conversation", &detail));
        }
        self.service
            .record_input(&record)
            .map_err(|error| port_error("read the governing conversation", error))
    }
    fn prepare_request(
        &self,
        revision: u64,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<DeveloperRequestAdmission, DeveloperLoopError> {
        self.prepare_request_for_role(DeveloperModelRole::Writer, revision, None, request)
    }

    fn prepare_role_request(
        &self,
        role: DeveloperModelRole,
        revision: u64,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<DeveloperRequestAdmission, DeveloperLoopError> {
        self.prepare_request_for_role(role, revision, None, request)
    }

    fn prepare_selected_role_request(
        &self,
        role: DeveloperModelRole,
        revision: u64,
        selection_provenance: Option<Sha256Digest>,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<DeveloperRequestAdmission, DeveloperLoopError> {
        self.prepare_request_for_role(role, revision, selection_provenance, request)
    }

    fn materialize_request(
        &self,
        request: peritus_model_protocol::ModelRequest,
    ) -> Result<peritus_model_protocol::ModelRequest, DeveloperLoopError> {
        let start = self.workbench_start()?;
        request.resolve_artifacts(|artifact, digest, maximum| {
            self.service
                .with_control_conversation(start.conversation(), |store| {
                    store.materialize_image_media(&start, artifact, digest, maximum)
                })
                .map_err(|error| {
                    port_error(
                        "resolve admitted image artifact for the selected provider",
                        error.into(),
                    )
                })
        })
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
            .with_control_conversation(start.conversation(), |store| {
                store.complete_goal_request(&start, goal_role(role), request_id, usage)
            })
            .map_err(|error| {
                port_error("commit model request usage to durable control state", error.into())
            })?;
        self.update(|_, progress| {
            progress.complete_provider_request(usage);
            Ok(())
        })?;
        self.release_request_source_snapshot()
            .map_err(|error| port_error("release admitted conversation sources", error))?;
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
            .with_control_conversation(start.conversation(), |store| {
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
            .with_control_conversation(start.conversation(), |store| {
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
            DeveloperActivity::ModelWaiting { elapsed_seconds } => {
                progress.mark_event("provider still waiting");
                narration::waiting(options, elapsed_seconds)
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
mod conversation;
#[cfg(not(verus_only))]
mod checkpoint_wait;
#[cfg(not(verus_only))]
mod request;
mod review;
pub(in crate::product_run::interaction) mod waiting;

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
        | peritus_product_runner::control::GoalAdmission::Inactive => DeveloperControlFlow::Stop,
    }
}
#[cfg(not(verus_only))]
fn port_error(operation: &'static str, error: ProductRunServiceError) -> DeveloperLoopError {
    DeveloperLoopError::Trace(format!("{operation}: {error}"))
}

#[cfg(not(verus_only))]
fn port_internal(operation: &'static str, detail: &str) -> DeveloperLoopError {
    DeveloperLoopError::Trace(format!("{operation}: {detail}"))
}
