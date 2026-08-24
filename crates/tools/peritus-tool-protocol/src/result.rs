//! Truthful closed terminal result envelopes.

mod accessors;
mod wire;

use crate::{
    ArtifactReference, BoundedJson, BoundedText, PreparedToolCall, ProtocolError,
    ProtocolErrorKind, ReplayIdentity, SchemaDigest,
};
use peritus_policy::AuthorityInstant;
use peritus_types::{ActionId, Sha256Digest};

/// Closed terminal status independent from prose.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResultStatus {
    /// Structured operation success.
    Succeeded,
    /// Operation or infrastructure failure.
    Failed,
    /// Router-mediated cancellation.
    Cancelled,
    /// Immutable call deadline elapsed.
    TimedOut,
    /// Outcome cannot safely be inferred or replayed.
    Indeterminate,
}

/// Stable failure category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureCategory {
    /// Invalid tool input or result envelope.
    Protocol,
    /// Authority or policy rejection.
    Authorization,
    /// Workspace target rejection.
    Workspace,
    /// Process/sandbox target rejection.
    Execution,
    /// Artifact publication was incomplete.
    Artifact,
    /// Required enforcement or service infrastructure was unavailable.
    Infrastructure,
    /// Cancellation completed without success.
    Cancelled,
    /// Deadline handling completed without success.
    Timeout,
    /// Recovery could not establish the effect outcome.
    Indeterminate,
}

/// Stable subsystem responsible for a failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponsibleSubsystem {
    /// C4 protocol validation.
    Protocol,
    /// C4 routing/authorization.
    Router,
    /// C1 workspace boundary.
    Workspace,
    /// C2 process boundary.
    Process,
    /// C3 sandbox/network/secret boundary.
    Sandbox,
    /// Artifact persistence/publication.
    ArtifactStore,
    /// Concrete tool implementation.
    Tool,
}

/// Stable retry statement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Retryability {
    /// Retrying this exact action is never allowed.
    Never,
    /// A new action and fresh authority may be attempted.
    NewAction,
    /// Recovery must decide before any retry.
    AfterRecovery,
}

/// Honest recovery route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryRoute {
    /// No recovery is applicable.
    None,
    /// Prepare and authorize a new action.
    Reauthorize,
    /// Reconcile the workspace target.
    ReconcileWorkspace,
    /// Reconcile an owned process record.
    ReconcileProcess,
    /// Republish an already-produced artifact.
    RepublishArtifact,
    /// Require authenticated human handling.
    HumanReview,
}

/// Complete stable failure value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolFailure {
    category: FailureCategory,
    code: BoundedText,
    subsystem: ResponsibleSubsystem,
    retryability: Retryability,
    recovery: RecoveryRoute,
    detail: BoundedText,
}

impl ToolFailure {
    /// Creates a typed failure independent from terminal rendering.
    #[must_use]
    pub const fn new(
        category: FailureCategory,
        code: BoundedText,
        subsystem: ResponsibleSubsystem,
        retryability: Retryability,
        recovery: RecoveryRoute,
        detail: BoundedText,
    ) -> Self {
        Self { category, code, subsystem, retryability, recovery, detail }
    }
    /// Returns the failure category.
    #[must_use]
    pub const fn category(&self) -> FailureCategory {
        self.category
    }
    /// Borrows the stable code.
    #[must_use]
    pub const fn code(&self) -> &BoundedText {
        &self.code
    }
    /// Returns the responsible subsystem.
    #[must_use]
    pub const fn subsystem(&self) -> ResponsibleSubsystem {
        self.subsystem
    }
    /// Returns retry semantics.
    #[must_use]
    pub const fn retryability(&self) -> Retryability {
        self.retryability
    }
    /// Returns the recovery route.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryRoute {
        self.recovery
    }
    /// Borrows bounded causal detail.
    #[must_use]
    pub const fn detail(&self) -> &BoundedText {
        &self.detail
    }

    /// Returns stable version-one canonical failure-envelope bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        wire::failure(self)
    }
}

/// Whether one rendering/output surface was truncated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Truncation {
    /// Complete bytes are present.
    Complete,
    /// Bytes were intentionally omitted at the end.
    TailDropped,
    /// Bytes were intentionally omitted at the beginning.
    HeadDropped,
    /// Bytes were omitted from both sides.
    Windowed,
    /// Completeness cannot be established.
    Indeterminate,
}

/// Independent truncation truth for output and renderings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TruncationMetadata {
    /// Complete structured/output stream truth.
    pub output: Truncation,
    /// Model rendering truth.
    pub model: Truncation,
    /// Human rendering truth.
    pub human: Truncation,
}

/// Authority-clock timing for one invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolTiming {
    started_at: AuthorityInstant,
    finished_at: AuthorityInstant,
}

impl ToolTiming {
    /// Creates non-regressing same-epoch timing.
    ///
    /// # Errors
    ///
    /// Rejects timing that regresses or crosses authority-clock epochs.
    pub fn new(
        started_at: AuthorityInstant,
        finished_at: AuthorityInstant,
    ) -> Result<Self, ProtocolError> {
        if started_at.epoch() != finished_at.epoch()
            || started_at.tick_millis() > finished_at.tick_millis()
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidEnvelope,
                "result.timing",
                "result timing regresses or crosses authority epochs",
            ));
        }
        Ok(Self { started_at, finished_at })
    }
    /// Returns start time.
    #[must_use]
    pub const fn started_at(self) -> AuthorityInstant {
        self.started_at
    }
    /// Returns terminal time.
    #[must_use]
    pub const fn finished_at(self) -> AuthorityInstant {
        self.finished_at
    }
}

/// Complete terminal result bound to the exact prepared call and replay identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolResult {
    action_id: ActionId,
    descriptor_digest: SchemaDigest,
    prepared_digest: Sha256Digest,
    replay_identity: ReplayIdentity,
    status: ResultStatus,
    structured: Option<BoundedJson>,
    failure: Option<ToolFailure>,
    human_rendering: BoundedText,
    model_rendering: BoundedText,
    artifacts: Vec<ArtifactReference>,
    timing: ToolTiming,
    truncation: TruncationMetadata,
    progress_count: u32,
}

impl ToolResult {
    /// Creates a truthful success with mandatory structured output.
    ///
    /// # Errors
    ///
    /// Rejects rendering, artifact, provenance, or progress values outside call bounds.
    #[allow(clippy::too_many_arguments)]
    pub fn success(
        prepared: &PreparedToolCall,
        structured: BoundedJson,
        human_rendering: BoundedText,
        model_rendering: BoundedText,
        artifacts: Vec<ArtifactReference>,
        timing: ToolTiming,
        truncation: TruncationMetadata,
        progress_count: u32,
    ) -> Result<Self, ProtocolError> {
        Self::build(
            prepared,
            ResultStatus::Succeeded,
            Some(structured),
            None,
            human_rendering,
            model_rendering,
            artifacts,
            timing,
            truncation,
            progress_count,
        )
    }

    /// Creates a closed non-success result with a mandatory typed failure.
    ///
    /// # Errors
    ///
    /// Rejects a success status or any value outside call bounds and provenance.
    #[allow(clippy::too_many_arguments)]
    pub fn failure(
        prepared: &PreparedToolCall,
        status: ResultStatus,
        failure: ToolFailure,
        structured: Option<BoundedJson>,
        human_rendering: BoundedText,
        model_rendering: BoundedText,
        artifacts: Vec<ArtifactReference>,
        timing: ToolTiming,
        truncation: TruncationMetadata,
        progress_count: u32,
    ) -> Result<Self, ProtocolError> {
        if status == ResultStatus::Succeeded {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidEnvelope,
                "result.status",
                "failure envelope cannot claim success",
            ));
        }
        Self::build(
            prepared,
            status,
            structured,
            Some(failure),
            human_rendering,
            model_rendering,
            artifacts,
            timing,
            truncation,
            progress_count,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        prepared: &PreparedToolCall,
        status: ResultStatus,
        structured: Option<BoundedJson>,
        failure: Option<ToolFailure>,
        human_rendering: BoundedText,
        model_rendering: BoundedText,
        artifacts: Vec<ArtifactReference>,
        timing: ToolTiming,
        truncation: TruncationMetadata,
        progress_count: u32,
    ) -> Result<Self, ProtocolError> {
        let limits = prepared.call().limits();
        let artifact_bytes = artifacts.iter().try_fold(0_u64, |total, artifact| {
            total.checked_add(artifact.size()).ok_or_else(|| {
                ProtocolError::at(
                    ProtocolErrorKind::InvalidEnvelope,
                    "result.artifacts",
                    "artifact byte total overflowed",
                )
            })
        })?;
        let structured_bytes = structured
            .as_ref()
            .map_or(0, |value| u64::try_from(value.canonical_bytes().len()).unwrap_or(u64::MAX));
        let total_output_bytes = artifact_bytes.saturating_add(structured_bytes);
        if artifacts.len() > limits.artifacts() as usize
            || progress_count > limits.progress_events()
            || human_rendering.as_str().len() > limits.human_bytes() as usize
            || model_rendering.as_str().len() > limits.model_bytes() as usize
            || total_output_bytes > limits.output_bytes()
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidEnvelope,
                "result",
                "terminal result exceeds call rendering, artifact, or progress bounds",
            ));
        }
        let action_id = prepared.call().action_id();
        if artifacts.iter().any(|artifact| {
            artifact.provenance().action_id() != action_id
                || artifact.provenance().prepared_digest() != prepared.prepared_digest()
        }) {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidEnvelope,
                "result.artifacts",
                "artifact provenance differs from the prepared call",
            ));
        }
        Ok(Self {
            action_id,
            descriptor_digest: prepared.descriptor_digest(),
            prepared_digest: prepared.prepared_digest(),
            replay_identity: prepared.replay_identity(),
            status,
            structured,
            failure,
            human_rendering,
            model_rendering,
            artifacts,
            timing,
            truncation,
            progress_count,
        })
    }
}
