//! Typed infrastructure and contract failures.

use std::error::Error;
use std::fmt;

use crate::{
    CorruptTarget, DependencyKind, EvidenceKind, FaultInjection, QualificationText,
    RecoveryOutcome, ScenarioId,
};

/// Stable subject-side error category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubjectErrorCode {
    /// Isolated subject creation failed.
    Setup,
    /// Fault control was unavailable or rejected.
    FaultControl,
    /// Storage or durable state operation failed.
    Persistence,
    /// Process, provider, tool, or worker supervision failed.
    Supervision,
    /// Restart or reconciliation failed.
    Recovery,
    /// Observation could not be collected or validated.
    Observation,
    /// Cleanup could not account for all owned resources.
    Cleanup,
    /// The implementation does not support a required H1 control.
    Unsupported,
}

/// Bounded error returned by an implementation adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubjectError {
    code: SubjectErrorCode,
    context: QualificationText,
    retryable: bool,
}

impl SubjectError {
    /// Creates a typed bounded subject error.
    #[must_use]
    pub const fn new(code: SubjectErrorCode, context: QualificationText, retryable: bool) -> Self {
        Self { code, context, retryable }
    }

    /// Returns the stable category.
    #[must_use]
    pub const fn code(&self) -> SubjectErrorCode {
        self.code
    }
    /// Returns bounded redacted context.
    #[must_use]
    pub const fn context(&self) -> &QualificationText {
        &self.context
    }
    /// Returns whether policy may retry this adapter operation.
    #[must_use]
    pub const fn retryable(&self) -> bool {
        self.retryable
    }
}

impl fmt::Display for SubjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "resilience subject {:?}: {}", self.code, self.context)
    }
}

impl Error for SubjectError {}

/// Runner stage containing a panic or subject error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailurePhase {
    /// Factory metadata inspection.
    Definition,
    /// Fresh subject creation.
    Setup,
    /// Active baseline preparation.
    Preparation,
    /// Fault arming and injection.
    Injection,
    /// Restart and reconciliation.
    Recovery,
    /// Owned cleanup.
    Cleanup,
}

/// Deterministic panic record; payloads and backtraces are intentionally excluded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PanicFailure {
    phase: FailurePhase,
}

impl PanicFailure {
    pub(crate) const fn new(phase: FailurePhase) -> Self {
        Self { phase }
    }

    /// Returns the stage that unwound.
    #[must_use]
    pub const fn phase(self) -> FailurePhase {
        self.phase
    }
}

/// Resource dimension whose configured ceiling was exceeded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceKind {
    /// Journal/trace/evidence events.
    Events,
    /// Retained evidence bytes.
    EvidenceBytes,
    /// Simultaneously owned processes.
    OwnedProcesses,
    /// Cleanup operations.
    CleanupSteps,
    /// Runtime-neutral logical time.
    LogicalTicks,
    /// Startup recovery decision steps.
    ReconciliationSteps,
    /// Per-scenario milestones.
    Milestones,
}

/// Direct-observation invariant violated by a scenario.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContractViolation {
    /// An observation identified a different scenario.
    ScenarioIdentityMismatch {
        /// Scenario selected by the runner.
        expected: ScenarioId,
        /// Scenario identified by the subject observation.
        observed: ScenarioId,
    },
    /// The subject reported a different injected fault.
    FaultIdentityMismatch {
        /// Fault selected by the scenario.
        expected: FaultInjection,
        /// Fault the subject reported arming.
        observed: FaultInjection,
    },
    /// The prepared baseline already claimed acceptance.
    BaselineAlreadyAccepted,
    /// The requested failpoint or disruption was never reached.
    FaultNotReached,
    /// Recovery classification differed from the documented outcome.
    UnexpectedRecovery {
        /// Recovery required by the scenario contract.
        expected: RecoveryOutcome,
        /// Recovery classification reported by the subject.
        observed: RecoveryOutcome,
    },
    /// Recovery created an accepted terminal state.
    FalseSuccess,
    /// Non-accepted state nevertheless claimed current acceptance evidence.
    ContradictoryAcceptanceEvidence,
    /// Crash recovery did not retain a healthy/recovered journal.
    CrashJournalDivergence,
    /// Corruption detection did not identify the exact injected target.
    CorruptionNotDetected {
        /// State target intentionally corrupted by the scenario.
        expected: CorruptTarget,
        /// Target diagnosed by the subject, if any.
        observed: Option<CorruptTarget>,
    },
    /// A non-corruption scenario unexpectedly diagnosed corrupt state.
    UnexpectedCorruption {
        /// State target unexpectedly diagnosed as corrupt.
        observed: CorruptTarget,
    },
    /// Mutation remained admitted after authoritative corrupt state was detected.
    MutationAdmittedWithCorruption,
    /// A corrupt projection was not rebuilt and verified.
    ProjectionNotRebuilt,
    /// Referenced objects were not verified after recovery.
    ReferencedObjectUnverified,
    /// Temporary disk-fault objects remained after reconciliation.
    TemporaryObjectLeak {
        /// Temporary objects still present after recovery.
        count: u16,
    },
    /// Startup recovery omitted its ownership/orphan scan.
    OwnershipScanMissing,
    /// Owned-work accounting overflowed or did not conserve the discovered count.
    OwnershipAccountingInvalid,
    /// Reconciliation left authoritative work unaccounted.
    UnaccountedWork {
        /// Owned items without a truthful reconciliation outcome.
        count: u16,
    },
    /// Reconciliation left an actual orphan outside ownership.
    OrphanedWork {
        /// Work items remaining outside authoritative ownership.
        count: u16,
    },
    /// A death/reboot case did not exercise any outstanding owned work.
    NoOwnedWorkExercised,
    /// A retry dimension exceeded its configured bound.
    RetryLimitExceeded {
        /// Dependency whose governed retries exceeded the bound.
        dependency: DependencyKind,
        /// Retry count reported by the subject.
        observed: u16,
        /// Configured retry ceiling.
        limit: u16,
    },
    /// A retry-exhaustion case did not consume exactly its governed ceiling.
    RetryExhaustionNotReached {
        /// Dependency whose exhaustion path was under qualification.
        dependency: DependencyKind,
        /// Retry count reported by the subject.
        observed: u16,
        /// Retry count required to demonstrate exhaustion.
        limit: u16,
    },
    /// A deterministic resource dimension exceeded its configured bound.
    ResourceLimitExceeded {
        /// Resource dimension whose ceiling was exceeded.
        resource: ResourceKind,
        /// Resource consumption reported by the subject.
        observed: u64,
        /// Configured resource ceiling.
        limit: u64,
    },
    /// A required evidence class was absent.
    MissingEvidence(EvidenceKind),
    /// Evidence anchors reused one kind or identifier.
    DuplicateEvidence,
    /// Lifecycle milestones were not the exact canonical six-step sequence.
    NonCanonicalMilestones,
    /// Cleanup did not release and account for every owned resource.
    CleanupIncomplete,
}

/// One scenario-level failure; no variant represents success.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScenarioFailure {
    /// Adapter operation returned an error.
    Subject {
        /// Runner stage that returned the adapter error.
        phase: FailurePhase,
        /// Typed bounded adapter error.
        error: SubjectError,
    },
    /// Adapter callback or future unwound.
    Panic(PanicFailure),
    /// Direct observations violated an H1 invariant.
    Contract(ContractViolation),
}

/// Definition failure that prevents a trustworthy suite verdict.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SuiteFailure {
    /// Factory metadata callback unwound.
    SubjectDescriptorPanic(PanicFailure),
    /// Catalog size exceeded the invocation bound.
    CatalogExceedsConfiguration {
        /// Number of scenarios in the selected catalog.
        actual: usize,
        /// Configured scenario-count ceiling.
        maximum: u16,
    },
}
