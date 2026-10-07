//! Live D0 input, provider selection, admission and typed activity adapter.

use super::{
    InteractionOptions, ProductRunService, ProductRunServiceError, narration, tool_activity,
};
use super::super::GoverningStateUnavailable;
use crate::product_control::{ControlStore, ControlStoreError, RequestSourceSnapshot};
use peritus_agent::DeveloperInput;
#[cfg(not(verus_only))]
use peritus_agent::{
    DeveloperActivity, DeveloperControlFlow, DeveloperInteraction, DeveloperLoopError,
    DeveloperModelRole, DeveloperProviderSelection, DeveloperRequestAdmission,
    DeveloperToolEffect,
};
use peritus_app_protocol::{ProductActivityKind, ProductModelChoice};
use peritus_product_runner::control::HostPermissions;
use peritus_types::{ProviderProfileId, RunId, Sha256Digest};
use std::{path::PathBuf, pin::Pin, sync::Arc, time::Duration};

const GOVERNING_STATE_RETRY_DELAY: Duration = Duration::from_millis(50);

#[derive(Clone)]
pub(super) struct GoverningState {
    revision: u64,
    incorporated: u64,
    conversation: String,
    stable_context: String,
    reference_authority: String,
    request_sources: Arc<RequestSourceSnapshot>,
    images: Vec<peritus_model_protocol::MediaInput>,
    protected_paths: Vec<PathBuf>,
    permissions: HostPermissions,
    permits_pipeline_handoff: bool,
}

pub(super) struct LiveConversation {
    pub(super) service: ProductRunService,
    pub(super) run_id: RunId,
    pub(super) start: peritus_product_runner::control::ControlOperation,
    pub(super) attempt_cancelled: Arc<std::sync::atomic::AtomicBool>,
    pub(super) request_sources: std::sync::Mutex<Option<Arc<RequestSourceSnapshot>>>,
    pub(super) governing_recovery: Arc<crate::product_control::ControlReconciliation>,
    pub(super) authoritative_revision: std::sync::atomic::AtomicU64,
    pub(super) governing: std::sync::RwLock<Option<GoverningState>>,
    pub(super) governing_unavailable: std::sync::RwLock<Option<GoverningStateUnavailable>>,
}
impl LiveConversation {
    pub(super) fn open(
        service: ProductRunService,
        run_id: RunId,
    ) -> Result<Arc<Self>, ProductRunServiceError> {
        let (attempt_cancelled, start, authoritative_revision) = service
            .inner
            .records
            .read()
            .map_err(|_| ProductRunServiceError::Unavailable)?
            .get(&run_id)
            .map(|record| {
                (
                    Arc::clone(&record.cancelled),
                    record.interaction.workbench.clone(),
                    record.interaction.incorporated,
                )
            })
            .ok_or(ProductRunServiceError::NotFound)?;
        let governing_recovery =
            service.retained_control_reconciliation(run_id, &attempt_cancelled)?;
        let conversation = Arc::new(Self {
            service,
            run_id,
            start,
            attempt_cancelled,
            request_sources: std::sync::Mutex::new(None),
            governing_recovery,
            authoritative_revision: std::sync::atomic::AtomicU64::new(
                authoritative_revision,
            ),
            governing: std::sync::RwLock::new(None),
            governing_unavailable: std::sync::RwLock::new(None),
        });
        conversation.initialize_governing_state()?;
        Ok(conversation)
    }

    pub(super) fn initialize_governing_state(&self) -> Result<(), ProductRunServiceError> {
        self.refresh_governing_state().map(drop)
    }

    pub(super) fn retained_governing_state(
        &self,
    ) -> Result<GoverningState, ProductRunServiceError> {
        self.governing
            .read()
            .map_err(|_| {
                ProductRunServiceError::internal(
                    "read retained governing state",
                    "the governing-state lock was poisoned",
                )
            })?
            .clone()
            .ok_or(ProductRunServiceError::InvalidState)
    }

    pub(super) fn projected_governing_state(&self) -> GoverningState {
        self.refresh_governing_state().unwrap_or_else(|error| {
            crate::diagnostic::report(&format!(
                "peritusd: governing-state refresh for {:?} stopped: {error}; retaining the last authoritative projection",
                self.run_id,
            ));
            self.retained_governing_state().unwrap_or_else(|retained_error| {
                panic!(
                    "live conversation exposed without an authoritative governing projection: {retained_error}"
                )
            })
        })
    }

    pub(super) fn refresh_governing_state(
        &self,
    ) -> Result<GoverningState, ProductRunServiceError> {
        let record = self.attempt_record()?;
        let start = record.interaction.workbench.clone();
        let workspace = record.request.workspace_id();
        let incorporated = record.interaction.incorporated;
        let host = self
            .service
            .permission_host(workspace)
            .map_err(ProductRunServiceError::from)?;
        let source = record
            .continuation_sources
            .iter()
            .rev()
            .find(|source| !source.settled)
            .copied();
        let state = self.with_governing_control("capture the governing conversation", |store| {
            let captured = source.map_or_else(
                || store.capture_execution(&start),
                |source| {
                    if incorporated < source.generation {
                        store.capture_execution_revision(&start, source.revision)
                    } else {
                        store.capture_execution_revision_incorporated(&start, source.revision)
                    }
                },
            )?;
            let control = store
                .load(start.conversation())?
                .ok_or(peritus_product_runner::control::ControlError::NotFound)?;
            if control.owner_bytes() != start.actor_bytes()
                || control.workspace_bytes() != start.workspace_bytes()
            {
                return Err(peritus_product_runner::control::ControlError::ScopeMismatch.into());
            }
            let stable_context = control.inputs().incorporated_conversation()?;
            let stable_context = if stable_context.is_empty() {
                "Current user instructions are supplied by the host at the request admission boundary."
                    .to_owned()
            } else {
                stable_context
            };
            let branch = store.branch(start.conversation())?;
            let host = super::super::permissions::branch_permissions(host, branch.as_ref());
            let permissions = store
                .permission_policy(workspace)?
                .effective_permissions(host);
            let permits_pipeline_handoff = control
                .reviews()
                .pending_pipeline_permission(control.inputs())?;
            let request_sources = Arc::new(store.request_source_snapshot(&captured)?);
            Ok(GoverningState {
                revision: captured.inputs().generation(),
                incorporated,
                conversation: captured.conversation_with_guidance()?,
                stable_context,
                reference_authority: captured.reference_authority_context().to_owned(),
                request_sources,
                images: captured.images().to_vec(),
                protected_paths: control.reviews().protected_paths(),
                permissions,
                permits_pipeline_handoff,
            })
        })?;
        *self.governing.write().map_err(|_| {
            ProductRunServiceError::internal(
                "retain governing state",
                "the governing-state lock was poisoned",
            )
        })? = Some(state.clone());
        self.authoritative_revision
            .store(state.revision, std::sync::atomic::Ordering::Release);
        Ok(state)
    }

    pub(super) fn with_governing_control<T>(
        &self,
        operation: &'static str,
        mut action: impl FnMut(&mut ControlStore) -> Result<T, ControlStoreError>,
    ) -> Result<T, ProductRunServiceError> {
        loop {
            match self.service.with_control_reconciliation(
                &self.governing_recovery,
                |store| action(store),
            ) {
                Ok(value) => {
                    let recovered = self
                        .governing_unavailable
                        .write()
                        .map_err(|_| ProductRunServiceError::Unavailable)?
                        .take();
                    if recovered.is_some() {
                        crate::diagnostic::report(&format!(
                            "peritusd: governing state for {:?} recovered at the retained control revision",
                            self.run_id,
                        ));
                    }
                    return Ok(value);
                }
                Err(error) if governing_storage_unavailable(&error) => {
                    let authoritative_revision = self
                        .authoritative_revision
                        .load(std::sync::atomic::Ordering::Acquire);
                    let unavailable = GoverningStateUnavailable::new(
                        self.run_id,
                        self.start.clone(),
                        authoritative_revision,
                        operation,
                        error,
                        Arc::clone(&self.governing_recovery),
                    );
                    let mut retained = self
                        .governing_unavailable
                        .write()
                        .map_err(|_| ProductRunServiceError::Unavailable)?;
                    if retained.is_none() {
                        crate::diagnostic::report(&format!(
                            "peritusd: {unavailable}; the exact recovery owner remains active",
                        ));
                    }
                    *retained = Some(unavailable.clone());
                    drop(retained);
                    if self
                        .attempt_cancelled
                        .load(std::sync::atomic::Ordering::Acquire)
                    {
                        return Err(ProductRunServiceError::GoverningStateUnavailable(
                            unavailable,
                        ));
                    }
                    std::thread::sleep(GOVERNING_STATE_RETRY_DELAY);
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

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

    pub(super) fn projected_request_source_snapshot(&self) -> Arc<RequestSourceSnapshot> {
        self.request_source_snapshot().unwrap_or_else(|error| {
            crate::diagnostic::report(&format!(
                "peritusd: admitted request-source projection for {:?} stopped: {error}; retaining its last authoritative binding",
                self.run_id,
            ));
            self.projected_governing_state().request_sources
        })
    }

    /// Reconciles the cached provider snapshot with the exact current durable input generation.
    /// Source tools use `request_source_snapshot` and therefore retain their admitted view;
    /// obligation boundaries call this method before adopting a conversation revision.
    pub(super) fn current_request_source_snapshot(
        &self,
    ) -> Result<Arc<RequestSourceSnapshot>, ProductRunServiceError> {
        let candidate = self.refresh_governing_state()?.request_sources;
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
        let state = self
            .refresh_governing_state()
            .map_err(|error| port_error("read the governing conversation", error))?;
        Ok(DeveloperInput {
            revision: state.revision,
            conversation: state.conversation,
            images: state.images,
        })
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
            self.with_governing_control(
                "resolve admitted image artifact for the selected provider",
                |store| {
                    store.materialize_image_media(&start, artifact, digest, maximum)
                },
            )
            .map_err(|error| {
                port_error(
                    "resolve admitted image artifact for the selected provider",
                    error,
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
            .with_governing_control(
                "commit model request usage to durable control state",
                |store| {
                store.complete_goal_request(&start, goal_role(role), request_id, usage)
                },
            )
            .map_err(|error| {
                port_error("commit model request usage to durable control state", error)
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
            .with_governing_control("reserve a tool call in durable control state", |store| {
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
                port_error("reserve a tool call in durable control state", error)
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
            .with_governing_control(
                "commit a tool result to durable control state",
                |store| {
                store.complete_goal_tool(&start, goal_role(role), invocation, sequence)
                },
            )
            .map_err(|error| {
                port_error("commit a tool result to durable control state", error)
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
    match error {
        ProductRunServiceError::GoverningStateUnavailable(unavailable) => {
            DeveloperLoopError::RecoveryRequired(format!("{operation}: {unavailable}"))
        }
        error => DeveloperLoopError::Trace(format!("{operation}: {error}")),
    }
}

#[cfg(not(verus_only))]
fn port_internal(operation: &'static str, detail: &str) -> DeveloperLoopError {
    DeveloperLoopError::Trace(format!("{operation}: {detail}"))
}

fn governing_storage_unavailable(error: &ControlStoreError) -> bool {
    matches!(
        error,
        ControlStoreError::Journal(_)
            | ControlStoreError::Io(_)
            | ControlStoreError::Corrupt(_)
    )
}
