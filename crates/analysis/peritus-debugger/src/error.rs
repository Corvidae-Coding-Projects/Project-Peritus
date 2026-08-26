//! Redaction-safe typed failures for debugger domain and effect boundaries.

use core::fmt;

/// Stable failure classification exposed by E2.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DebuggerErrorKind {
    /// A caller supplied an invalid identity, value, order, or bound.
    InvalidInput,
    /// Cross-slice run, attempt, revision, environment, or harness facts disagree.
    Binding,
    /// Trace evidence could not be selected completely and deterministically.
    Selection,
    /// A citation is absent, outside the manifest, or bound to other evidence.
    Citation,
    /// A failure classification is unknown or structurally invalid.
    Taxonomy,
    /// A report or claim violates the checked report contract.
    Report,
    /// C5 request, stream, or normalized response processing failed.
    ModelProtocol,
    /// Model output was inert but did not pass E2 validation.
    ModelRejected,
    /// A compiled or job-specific resource ceiling was exceeded.
    Budget,
    /// Durable or cooperative cancellation won.
    Cancelled,
    /// A command is not legal in the current durable phase.
    IllegalTransition,
    /// A stable command or domain identity was reused for different bytes.
    IdempotencyConflict,
    /// C0 journal access, append, or integrity validation failed.
    Journal,
    /// C0 artifact finalization, verification, or bounded reading failed.
    Artifact,
    /// C0 evidence admission or lookup failed.
    Evidence,
    /// Persistent schema migration or compatibility validation failed.
    Migration,
    /// Restart state is valid only after replay or effect reconciliation.
    Recovery,
    /// Durable bytes, hashes, chains, or checkpoints are inconsistent.
    Corruption,
}

/// Operation at which an E2 failure was detected.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DebuggerOperation {
    /// Validate a nominal identity or immutable binding.
    ValidateBinding,
    /// Validate a frozen selection query and its limits.
    ValidateQuery,
    /// Select and freeze C7 evidence.
    SelectEvidence,
    /// Build one deterministic per-attempt timeline.
    BuildTimeline,
    /// Derive causal candidates and alternatives.
    AnalyzeCauses,
    /// Cluster cross-run patterns.
    ClusterPatterns,
    /// Map patterns to immutable E1 declarations.
    MapComponents,
    /// Validate a source event or artifact citation.
    ValidateCitation,
    /// Validate or encode a diagnostic report.
    ValidateReport,
    /// Build or execute a provider-neutral C5 analysis request.
    RunModelAnalysis,
    /// Apply a pure debugger aggregate transition.
    ApplyTransition,
    /// Encode or decode a B3 debugger frame.
    DecodeProtocol,
    /// Commit one event/checkpoint/outbox transaction.
    CommitTransition,
    /// Load and replay durable E2 state.
    Replay,
    /// Reconcile pending work after restart.
    Recover,
    /// Finalize or verify a report artifact.
    PublishArtifact,
    /// Admit a report through the C0 evidence catalog.
    PublishEvidence,
    /// Upgrade or validate persistent schema compatibility.
    Migrate,
}

/// Stable caller action appropriate for a typed failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DebuggerRecovery {
    /// Correct the immutable request or supplied value.
    CorrectInput,
    /// Repair or restore a required upstream C0/C7/E0/D0/E1 dependency.
    RepairDependency,
    /// Replay the complete debugger aggregate before deciding again.
    ReplayAggregate,
    /// Retry the same idempotent operation within its durable budget.
    Retry,
    /// Reconcile durable outbox, artifact, or evidence state before retrying.
    Reconcile,
    /// Isolate the aggregate and require an operator repair decision.
    Quarantine,
    /// Cancellation is terminal and requires no recovery action.
    None,
}

/// One redaction-safe E2 failure.
#[derive(Clone, Eq, PartialEq)]
pub struct DebuggerError {
    kind: DebuggerErrorKind,
    operation: DebuggerOperation,
    recovery: DebuggerRecovery,
    detail: String,
    expected: Option<u64>,
    actual: Option<u64>,
}

impl DebuggerError {
    /// Creates a typed failure with a redaction-safe detail.
    #[must_use]
    pub fn new(
        kind: DebuggerErrorKind,
        operation: DebuggerOperation,
        recovery: DebuggerRecovery,
        detail: impl Into<String>,
    ) -> Self {
        Self { kind, operation, recovery, detail: detail.into(), expected: None, actual: None }
    }

    /// Creates a typed bound or numeric mismatch without embedding source content.
    #[must_use]
    pub fn numbers(
        kind: DebuggerErrorKind,
        operation: DebuggerOperation,
        recovery: DebuggerRecovery,
        detail: impl Into<String>,
        expected: u64,
        actual: u64,
    ) -> Self {
        Self {
            kind,
            operation,
            recovery,
            detail: detail.into(),
            expected: Some(expected),
            actual: Some(actual),
        }
    }

    /// Returns the stable failure kind.
    #[must_use]
    pub const fn kind(&self) -> DebuggerErrorKind {
        self.kind
    }

    /// Returns the failing operation.
    #[must_use]
    pub const fn operation(&self) -> DebuggerOperation {
        self.operation
    }

    /// Returns the stable recovery class.
    #[must_use]
    pub const fn recovery(&self) -> DebuggerRecovery {
        self.recovery
    }

    /// Borrows the redaction-safe diagnostic detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    /// Returns an expected bound or value when present.
    #[must_use]
    pub const fn expected(&self) -> Option<u64> {
        self.expected
    }

    /// Returns the observed bound or value when present.
    #[must_use]
    pub const fn actual(&self) -> Option<u64> {
        self.actual
    }
}

impl fmt::Debug for DebuggerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DebuggerError")
            .field("kind", &self.kind)
            .field("operation", &self.operation)
            .field("recovery", &self.recovery)
            .field("detail", &self.detail)
            .field("expected", &self.expected)
            .field("actual", &self.actual)
            .finish()
    }
}

impl fmt::Display for DebuggerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?} while {:?}: {}", self.kind, self.operation, self.detail)?;
        if let (Some(expected), Some(actual)) = (self.expected, self.actual) {
            write!(formatter, " (expected {expected}, observed {actual})")?;
        }
        Ok(())
    }
}

impl std::error::Error for DebuggerError {}
