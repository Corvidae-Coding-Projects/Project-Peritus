//! Stable typed F0 failures and recovery guidance.

/// Stable evolution failure class.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EvolutionErrorKind {
    /// Input cannot represent a valid domain value.
    InvalidInput,
    /// Immutable identities or digests do not match.
    BindingDrift,
    /// Input or state exceeds a declared bound.
    LimitExceeded,
    /// Collection order or uniqueness is invalid.
    NonCanonical,
    /// Evidence is absent, incomplete, or unavailable.
    IncompleteEvidence,
    /// Candidate or evidence crossed a sealed boundary.
    Contamination,
    /// Frozen promotion policy rejected the candidate.
    PolicyRejected,
    /// Command is illegal in the current phase.
    IllegalTransition,
    /// Expected head, generation, or baseline is stale.
    StaleState,
    /// Checked arithmetic could not represent the result.
    Arithmetic,
    /// Retained state is internally contradictory.
    Corruption,
    /// Canonical protocol bytes are malformed or unsupported.
    Codec,
    /// The durable journal rejected or could not complete an operation.
    Journal,
    /// A content-addressed artifact is absent or inconsistent.
    Artifact,
    /// Immutable evidence admission or lookup failed.
    Evidence,
    /// B0/B1 or human approval authority does not match the action.
    Authority,
}

/// Operation that rejected an F0 request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EvolutionOperation {
    /// Validate an F0 nominal identity.
    ValidateIdentity,
    /// Validate caller-selected limits.
    ValidateLimits,
    /// Capture a production harness binding.
    BindProductionHarness,
    /// Capture published E2 diagnosis.
    BindDiagnosis,
    /// Capture published E3 evaluation.
    BindEvaluation,
    /// Capture terminal D2 review.
    BindReview,
    /// Bind the protected evolution policy.
    BindPolicy,
    /// Validate a change manifest.
    AdmitManifest,
    /// Validate an isolated variant.
    AdmitVariant,
    /// Attribute declared predictions.
    Attribute,
    /// Evaluate policy and select a candidate.
    Select,
    /// Apply a campaign command.
    TransitionCampaign,
    /// Apply a production-pointer command.
    TransitionPointer,
    /// Validate rollback reachability and compatibility.
    Rollback,
    /// Encode or decode canonical protocol state.
    Codec,
    /// Commit an accepted durable transition.
    Commit,
    /// Reconstruct state from durable records.
    Recover,
    /// Finalize or publish immutable evidence.
    Publish,
    /// Validate and consume production mutation authority.
    Authorize,
}

/// Actionable recovery class.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EvolutionRecovery {
    /// Correct malformed or mismatched input.
    CorrectInput,
    /// Reduce caller-selected scope or bounds.
    ReduceScope,
    /// Refresh exact aggregate or production-pointer state.
    RefreshState,
    /// Obtain missing immutable evidence.
    ObtainEvidence,
    /// Create a successor campaign from new frozen inputs.
    SuccessorCampaign,
    /// Quarantine contradictory persisted data.
    Quarantine,
    /// Rejection is a truthful terminal policy result.
    Terminal,
    /// Retry the exact idempotent operation without changing its identity.
    Retry,
    /// Reconcile durable state with an external artifact or evidence owner.
    Reconcile,
    /// Replay canonical durable records to reconstruct the authoritative state.
    Replay,
    /// Obtain a new matching human authorization.
    RequestAuthority,
}

/// One bounded redaction-safe F0 error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvolutionError {
    kind: EvolutionErrorKind,
    operation: EvolutionOperation,
    recovery: EvolutionRecovery,
    detail: &'static str,
}

impl EvolutionError {
    /// Creates a typed error with static safe detail.
    #[must_use]
    pub const fn new(
        kind: EvolutionErrorKind,
        operation: EvolutionOperation,
        recovery: EvolutionRecovery,
        detail: &'static str,
    ) -> Self {
        Self { kind, operation, recovery, detail }
    }
    /// Returns the stable failure class.
    #[must_use]
    pub const fn kind(&self) -> EvolutionErrorKind {
        self.kind
    }
    /// Returns the rejected operation.
    #[must_use]
    pub const fn operation(&self) -> EvolutionOperation {
        self.operation
    }
    /// Returns actionable recovery guidance.
    #[must_use]
    pub const fn recovery(&self) -> EvolutionRecovery {
        self.recovery
    }
    /// Returns static redaction-safe detail.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}

impl core::fmt::Display for EvolutionError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.detail)
    }
}

impl std::error::Error for EvolutionError {}
