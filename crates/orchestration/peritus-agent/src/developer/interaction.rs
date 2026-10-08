//! Optional live user-input and public activity boundary for developer execution.

use super::DeveloperLoopError;
use std::pin::Pin;
pub use crate::developer_interaction::{DeveloperInput, DeveloperRequestAdmission};

/// Host-owned role whose model is selected at each new logical model turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeveloperModelRole {
    /// Conversation, design, and implementation model.
    Writer,
    /// Independent review model.
    Reviewer,
    /// Review remediation model.
    Fixer,
}

/// Exact accepted provider-request identity used by host-owned progress projections.
///
/// This is presentation metadata only. It cannot authorize a retry, continuation, or replacement
/// request, and it remains separate from the raw provider trace and accounting authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeveloperProviderRequestIdentity {
    role: DeveloperModelRole,
    turn: u16,
    attempt: u64,
    request_id_digest: peritus_types::Sha256Digest,
    request_fingerprint: peritus_types::Sha256Digest,
    provider_profile_id: peritus_types::ProviderProfileId,
    provider_profile_revision: u64,
    provider_name_digest: peritus_types::Sha256Digest,
    native_session_digest: Option<peritus_types::Sha256Digest>,
    provider_selection_digest: Option<peritus_types::Sha256Digest>,
}

impl DeveloperProviderRequestIdentity {
    /// Reconstructs an identity from exact typed durable fields.
    #[must_use]
    #[allow(clippy::too_many_arguments, reason = "the identity preserves independent authority axes")]
    pub const fn from_parts(
        role: DeveloperModelRole,
        turn: u16,
        attempt: u64,
        request_id_digest: peritus_types::Sha256Digest,
        request_fingerprint: peritus_types::Sha256Digest,
        provider_profile_id: peritus_types::ProviderProfileId,
        provider_profile_revision: u64,
        provider_name_digest: peritus_types::Sha256Digest,
        native_session_digest: Option<peritus_types::Sha256Digest>,
        provider_selection_digest: Option<peritus_types::Sha256Digest>,
    ) -> Self {
        Self {
            role,
            turn,
            attempt,
            request_id_digest,
            request_fingerprint,
            provider_profile_id,
            provider_profile_revision,
            provider_name_digest,
            native_session_digest,
            provider_selection_digest,
        }
    }

    #[must_use]
    pub const fn role(self) -> DeveloperModelRole { self.role }
    #[must_use]
    pub const fn turn(self) -> u16 { self.turn }
    #[must_use]
    pub const fn attempt(self) -> u64 { self.attempt }
    #[must_use]
    pub const fn request_id_digest(self) -> peritus_types::Sha256Digest {
        self.request_id_digest
    }
    #[must_use]
    pub const fn request_fingerprint(self) -> peritus_types::Sha256Digest {
        self.request_fingerprint
    }
    #[must_use]
    pub const fn provider_profile_id(self) -> peritus_types::ProviderProfileId {
        self.provider_profile_id
    }
    #[must_use]
    pub const fn provider_profile_revision(self) -> u64 { self.provider_profile_revision }
    #[must_use]
    pub const fn provider_name_digest(self) -> peritus_types::Sha256Digest {
        self.provider_name_digest
    }
    #[must_use]
    pub const fn native_session_digest(self) -> Option<peritus_types::Sha256Digest> {
        self.native_session_digest
    }
    #[must_use]
    pub const fn provider_selection_digest(self) -> Option<peritus_types::Sha256Digest> {
        self.provider_selection_digest
    }
}

/// One atomically resolved host provider choice plus optional explicit-selection provenance.
pub struct DeveloperProviderSelection {
    provider: std::sync::Arc<dyn peritus_provider_core::ModelProvider>,
    provenance: Option<peritus_types::Sha256Digest>,
}

impl DeveloperProviderSelection {
    /// Binds an adapter to the digest of the exact persisted user selection that resolved it.
    #[must_use]
    pub fn new(
        provider: std::sync::Arc<dyn peritus_provider_core::ModelProvider>,
        provenance: peritus_types::Sha256Digest,
    ) -> Self {
        Self { provider, provenance: Some(provenance) }
    }

    fn without_provenance(
        provider: std::sync::Arc<dyn peritus_provider_core::ModelProvider>,
    ) -> Self {
        Self { provider, provenance: None }
    }

    /// Borrows the immutable adapter resolved from this selection snapshot.
    #[must_use]
    pub fn provider(&self) -> &dyn peritus_provider_core::ModelProvider {
        self.provider.as_ref()
    }

    /// Consumes the snapshot and returns its immutable adapter.
    #[must_use]
    pub fn into_provider(self) -> std::sync::Arc<dyn peritus_provider_core::ModelProvider> {
        self.provider
    }

    /// Returns the exact persisted-selection digest when the host can prove it.
    #[must_use]
    pub const fn provenance(&self) -> Option<peritus_types::Sha256Digest> {
        self.provenance
    }
}

/// Host decision after an already-admitted provider or tool operation settles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeveloperControlFlow {
    /// The next operation may still be considered under current host control.
    Continue,
    /// Newer host input superseded the prepared operation; return to the next model boundary.
    Yield,
    /// Return at this safe boundary without admitting another operation.
    Stop,
}

/// Conservative effect class used for before-edit goal pauses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeveloperToolEffect {
    /// Inspection proven not to mutate workspace or external state.
    ReadOnly,
    /// Any operation that can mutate state or whose effect class is not proven read-only.
    MutationCapable,
}

/// Explicit owner of transcript compaction for one developer-loop invocation.
///
/// A local context owner selects and checkpoints the provider-visible frontier from its retained
/// lineage. The legacy loop operates only when no local port is present; its optional semantic
/// pass may be enabled independently of its deterministic complete-exchange compaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeveloperCompactionOwner {
    /// The supplied [`super::DeveloperContextPort`] owns selection and durable publication.
    LocalContext,
    /// D0 owns the legacy transcript and may optionally ask the task provider for a summary.
    LegacyLoop {
        /// Whether the legacy provider-authored semantic pass is permitted.
        provider_semantic: bool,
    },
}

impl DeveloperCompactionOwner {
    /// The default legacy owner, including its provider-authored semantic pass.
    pub const LEGACY: Self = Self::LegacyLoop { provider_semantic: true };

    /// Returns whether the legacy owner may dispatch a provider-authored semantic pass.
    #[must_use]
    pub const fn permits_provider_semantic(self) -> bool {
        matches!(self, Self::LegacyLoop { provider_semantic: true })
    }
}

/// Host-validated reason for repeating an independent review; contains no provider payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeveloperReviewRetryReason {
    /// The reviewer did not inspect the required repository evidence.
    MissingGrounding,
    /// The completed response did not satisfy the typed review contract.
    InvalidSubmission,
}

/// Public execution activity, separate from the raw durable provider trace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeveloperActivity<'a> {
    /// A model request is about to start.
    ModelStarted {
        /// Exact requested model identifier.
        model: &'a str,
        /// Reasoning control in the outgoing request, not an inferred provider outcome.
        reasoning: peritus_model_protocol::ReasoningPolicy,
        /// Exact admitted request, profile, provider, native-session, and retry-attempt identity.
        request: DeveloperProviderRequestIdentity,
    },
    /// No public text has arrived while a provider request remains pending.
    ModelWaiting { elapsed_seconds: u64 },
    /// A bounded syntax repair was durably recorded; no response contents are exposed here.
    ResponseHealed,
    /// A rejected independent review will be retried with fresh repository evidence.
    ReviewRetry {
        /// One-based attempt about to start.
        next_attempt: u64,
        /// Validated public reason; raw output remains in the private trace.
        reason: DeveloperReviewRetryReason,
    },
    /// Public assistant text received from a provider, never a reasoning delta.
    Text(&'a [u8]),
    /// Provider-supplied display summary, never opaque reasoning replay bytes.
    ReasoningSummary(&'a [u8]),
    /// One exact tool call is about to execute.
    ToolStarted { name: &'a str, arguments: &'a str },
    /// A tool completed with a bounded, user-visible result.
    ToolFinished { name: &'a str, output: &'a str, is_error: bool },
    /// A tool call was not executed because newer user input superseded it.
    ToolSkipped { name: &'a str },
}

/// Daemon-owned live input and observation port; it cannot grant tool authority.
pub trait DeveloperInteraction: Send + Sync {
    /// Declares the compaction owner required by this host invocation.
    ///
    /// The loop negotiates this value against the supplied context port before reopening any
    /// invocation. It never falls back from a missing local owner to legacy compaction.
    fn compaction_owner(&self) -> DeveloperCompactionOwner {
        DeveloperCompactionOwner::LEGACY
    }
    /// Resolves an immutable adapter for the next turn, without changing an in-flight request.
    /// Returning `None` retains the caller's fixed provider.
    ///
    /// # Errors
    /// Fails closed if the selected adapter cannot be resolved.
    fn provider(
        &self,
        _role: DeveloperModelRole,
    ) -> Result<Option<std::sync::Arc<dyn peritus_provider_core::ModelProvider>>, DeveloperLoopError>
    {
        Ok(None)
    }

    /// Atomically resolves the next-turn adapter and exact explicit-selection provenance.
    /// Hosts that cannot prove selection provenance retain compatibility but cannot authorize
    /// automatic retirement of a retry schedule after a provider-profile mismatch.
    ///
    /// # Errors
    /// Fails closed if the selected adapter or its durable provenance cannot be resolved.
    fn provider_selection(
        &self,
        role: DeveloperModelRole,
    ) -> Result<Option<DeveloperProviderSelection>, DeveloperLoopError> {
        self.provider(role)
            .map(|provider| provider.map(DeveloperProviderSelection::without_provenance))
    }

    /// Atomically captures current input. Failure stops rather than using a stale snapshot.
    ///
    /// # Errors
    /// Returns a durable-input or synchronization failure.
    fn input(&self) -> Result<DeveloperInput, DeveloperLoopError>;

    /// Atomically checks current input and durably binds it to the exact prepared request.
    /// Upgraded hosts publish the immutable input IDs/revisions and request fingerprint in
    /// the same transaction as incorporation. This method must not start provider execution.
    ///
    /// # Errors
    /// Returns a persistence failure; no request is sent after a failed acknowledgement.
    fn prepare_request(
        &self,
        revision: u64,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<DeveloperRequestAdmission, DeveloperLoopError>;

    /// Role-aware request admission. Existing hosts retain their prior behavior by default.
    /// Goal-aware hosts override this to reserve cumulative budget before provider dispatch.
    ///
    /// # Errors
    /// Returns a durable-input, control, or synchronization failure.
    fn prepare_role_request(
        &self,
        _role: DeveloperModelRole,
        revision: u64,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<DeveloperRequestAdmission, DeveloperLoopError> {
        self.prepare_request(revision, request)
    }

    /// Admits a request only while its explicit provider-selection snapshot is still current.
    /// Hosts without explicit selection provenance retain the legacy role-aware admission path.
    ///
    /// # Errors
    /// Returns a durable-input, selection, control, or synchronization failure.
    fn prepare_selected_role_request(
        &self,
        role: DeveloperModelRole,
        revision: u64,
        _selection_provenance: Option<peritus_types::Sha256Digest>,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<DeveloperRequestAdmission, DeveloperLoopError> {
        self.prepare_role_request(role, revision, request)
    }

    /// Resolves host-owned artifact media after durable request admission and before provider I/O.
    ///
    /// The default preserves requests that contain only inline or provider-owned media. A host
    /// that emits Peritus artifact references must return an exact transient provider projection.
    ///
    /// # Errors
    /// Returns a durable artifact, integrity, or selected-provider limit failure.
    fn materialize_request(
        &self,
        request: peritus_model_protocol::ModelRequest,
    ) -> Result<peritus_model_protocol::ModelRequest, DeveloperLoopError> {
        Ok(request)
    }

    /// Reconciles one admitted request at its terminal provider boundary.
    ///
    /// # Errors
    /// Returns a durable accounting or synchronization failure.
    fn complete_role_request(
        &self,
        _role: DeveloperModelRole,
        _request_id: &str,
        _usage: peritus_model_protocol::UsageCounters,
    ) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        Ok(DeveloperControlFlow::Continue)
    }

    /// Reserves one tool call before the executor can observe or perform its effect. `invocation`
    /// is the stable request prefix for this complete developer-loop invocation; `sequence` is
    /// scoped to it and may restart in a later invocation.
    ///
    /// # Errors
    /// Returns a durable control or synchronization failure.
    fn admit_tool(
        &self,
        _role: DeveloperModelRole,
        _invocation: &str,
        _sequence: u32,
        _input_revision: u64,
        _effect: DeveloperToolEffect,
    ) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        Ok(DeveloperControlFlow::Continue)
    }

    /// Reconciles one admitted tool at the next safe boundary using the same invocation identity
    /// and loop-local sequence supplied at admission.
    ///
    /// # Errors
    /// Returns a durable accounting or synchronization failure.
    fn complete_tool(
        &self,
        _role: DeveloperModelRole,
        _invocation: &str,
        _sequence: u32,
    ) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        Ok(DeveloperControlFlow::Continue)
    }

    /// Records safe public activity independently of raw trace storage.
    ///
    /// # Errors
    /// Returns an observation failure rather than claiming invisible successful progress.
    fn observe(&self, activity: DeveloperActivity<'_>) -> Result<(), DeveloperLoopError>;

    /// Delivers a coalescible waiting projection independently of provider ownership.
    /// Hosts may retain it in a durable outbox. Delivery failure is diagnostic and must never
    /// cancel, replace, or retry the already-admitted provider request.
    fn observe_waiting(
        &self,
        elapsed_seconds: u64,
    ) -> Pin<Box<dyn Future<Output = Result<(), DeveloperLoopError>> + Send + '_>> {
        Box::pin(async move {
            self.observe(DeveloperActivity::ModelWaiting { elapsed_seconds })
        })
    }

    /// Reports a waiting-projection failure without altering accepted execution state.
    fn waiting_observation_failed(&self, error: &DeveloperLoopError) {
        eprintln!("peritus: waiting status delivery failed; provider ownership is retained: {error}");
    }
}
