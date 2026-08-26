//! Closed schema-v1 outcome and failure taxonomy.

use crate::{DebuggerError, DebuggerErrorKind, DebuggerOperation, DebuggerRecovery};

/// Task-level outcome, distinct from failures of the execution infrastructure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TaskOutcome {
    /// The task completed successfully.
    Success,
    /// A deterministic requirement or gate assertion failed.
    RequirementFailure,
    /// The task could not proceed because a declared dependency or decision blocked it.
    Blocked,
    /// Task policy intentionally cancelled the attempt.
    CancelledByTaskPolicy,
    /// Evidence cannot determine a task outcome.
    Indeterminate,
}

/// Infrastructure-level outcome, never silently reclassified as task success.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InfrastructureOutcome {
    /// Provider boundary failed.
    ProviderFailure,
    /// Tool boundary failed.
    ToolFailure,
    /// Workspace or version-control boundary failed.
    WorkspaceFailure,
    /// Sandbox, process, network, or resource boundary failed.
    SandboxFailure,
    /// A gate could not execute or parse its result.
    GateInfrastructureFailure,
    /// Journal, artifact, projection, migration, or recovery failed.
    StorageFailure,
    /// Approval or authority boundary failed.
    AuthorityFailure,
    /// Scheduling or dependency execution failed.
    SchedulerFailure,
    /// Evidence cannot determine an infrastructure outcome.
    IndeterminateInfrastructure,
}

/// A normalized terminal outcome preserving the task/infrastructure boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OutcomeClass {
    /// Task semantics.
    Task(TaskOutcome),
    /// Delivery infrastructure semantics.
    Infrastructure(InfrastructureOutcome),
}

impl OutcomeClass {
    /// Returns whether this is a successful task outcome.
    #[must_use]
    pub const fn is_task_success(self) -> bool {
        matches!(self, Self::Task(TaskOutcome::Success))
    }

    /// Returns a stable schema-v1 tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Task(TaskOutcome::Success) => 1,
            Self::Task(TaskOutcome::RequirementFailure) => 2,
            Self::Task(TaskOutcome::Blocked) => 3,
            Self::Task(TaskOutcome::CancelledByTaskPolicy) => 4,
            Self::Task(TaskOutcome::Indeterminate) => 5,
            Self::Infrastructure(InfrastructureOutcome::ProviderFailure) => 101,
            Self::Infrastructure(InfrastructureOutcome::ToolFailure) => 102,
            Self::Infrastructure(InfrastructureOutcome::WorkspaceFailure) => 103,
            Self::Infrastructure(InfrastructureOutcome::SandboxFailure) => 104,
            Self::Infrastructure(InfrastructureOutcome::GateInfrastructureFailure) => 105,
            Self::Infrastructure(InfrastructureOutcome::StorageFailure) => 106,
            Self::Infrastructure(InfrastructureOutcome::AuthorityFailure) => 107,
            Self::Infrastructure(InfrastructureOutcome::SchedulerFailure) => 108,
            Self::Infrastructure(InfrastructureOutcome::IndeterminateInfrastructure) => 109,
        }
    }
}

/// Complete append-only schema-v1 failure category and subcategory catalog.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FailureCategory {
    /// Specification is ambiguous.
    SpecificationAmbiguity = 101,
    /// Specification requirements conflict.
    SpecificationConflict = 102,
    /// A requirement cannot be achieved under the frozen contract.
    SpecificationUnachievable = 103,
    /// Required context was not selected.
    ContextSelection = 201,
    /// Context compaction lost required meaning or binding.
    ContextCompaction = 202,
    /// Context provenance is absent or inconsistent.
    ContextProvenance = 203,
    /// Model reasoning produced an incorrect or unsupported result.
    ModelReasoning = 301,
    /// Model output was malformed.
    ModelMalformedOutput = 302,
    /// Model refused the requested inert analysis.
    ModelRefusal = 303,
    /// Model completion was missing, incomplete, or contradictory.
    ModelCompletion = 304,
    /// Provider authentication failed.
    ProviderAuthentication = 401,
    /// Provider quota was exhausted.
    ProviderQuota = 402,
    /// Provider rate-limited work.
    ProviderRateLimit = 403,
    /// Provider transport failed.
    ProviderTransport = 404,
    /// Provider protocol failed.
    ProviderProtocol = 405,
    /// Provider usage accounting disagreed.
    ProviderAccounting = 406,
    /// Tool schema was invalid or unsupported.
    ToolSchema = 501,
    /// Tool routing selected no valid implementation.
    ToolRouting = 502,
    /// Tool authorization was absent or rejected.
    ToolAuthorization = 503,
    /// Tool execution failed.
    ToolExecution = 504,
    /// Tool result normalization failed.
    ToolResultNormalization = 505,
    /// Workspace state or lifecycle disagreed.
    Workspace = 601,
    /// Patch construction or application failed.
    Patch = 602,
    /// Git state or operation failed.
    Git = 603,
    /// A path was invalid, conflicting, or escaped its boundary.
    PathConflict = 604,
    /// Sandbox setup or enforcement failed.
    Sandbox = 701,
    /// Process creation, observation, or teardown failed.
    Process = 702,
    /// Network boundary failed.
    Network = 703,
    /// A bounded resource was exhausted.
    Resource = 704,
    /// A deterministic gate assertion failed the task.
    DeterministicGateFailure = 801,
    /// Gate execution or result parsing infrastructure failed.
    GateInfrastructureFailure = 802,
    /// Reviewers disagreed without a resolved disposition.
    ReviewDisagreement = 901,
    /// A review finding was invalid.
    ReviewInvalidFinding = 902,
    /// A review blocker remained unresolved.
    ReviewUnresolvedBlocker = 903,
    /// Writer, reviewer, or fixer repeatedly oscillated.
    ReviewOscillation = 904,
    /// Journal durability or integrity failed.
    Journal = 1001,
    /// Artifact finalization, verification, or lookup failed.
    Artifact = 1002,
    /// A rebuildable projection disagreed or failed.
    Projection = 1003,
    /// Persistent schema migration failed.
    Migration = 1004,
    /// Recovery or reconciliation failed.
    Recovery = 1005,
    /// Approval or authority timed out.
    AuthorityTimeout = 1101,
    /// Approval or authority was denied.
    AuthorityDenied = 1102,
    /// Scheduler starvation prevented progress.
    SchedulerStarvation = 1201,
    /// Scheduler cancellation won.
    SchedulerCancellation = 1202,
    /// A scheduled dependency failed.
    SchedulerDependencyFailure = 1203,
    /// Evolution evidence was contaminated.
    EvolutionContamination = 1301,
    /// Attribution across revisions or components was uncertain.
    EvolutionAttributionUncertainty = 1302,
    /// A statistical evaluator rejected a candidate.
    EvolutionStatisticalRejection = 1303,
    /// A separate promotion authority denied advancement.
    EvolutionPromotionDenial = 1304,
}

impl FailureCategory {
    /// Complete schema-v1 catalog in numeric tag order.
    pub const ALL: [Self; 49] = [
        Self::SpecificationAmbiguity,
        Self::SpecificationConflict,
        Self::SpecificationUnachievable,
        Self::ContextSelection,
        Self::ContextCompaction,
        Self::ContextProvenance,
        Self::ModelReasoning,
        Self::ModelMalformedOutput,
        Self::ModelRefusal,
        Self::ModelCompletion,
        Self::ProviderAuthentication,
        Self::ProviderQuota,
        Self::ProviderRateLimit,
        Self::ProviderTransport,
        Self::ProviderProtocol,
        Self::ProviderAccounting,
        Self::ToolSchema,
        Self::ToolRouting,
        Self::ToolAuthorization,
        Self::ToolExecution,
        Self::ToolResultNormalization,
        Self::Workspace,
        Self::Patch,
        Self::Git,
        Self::PathConflict,
        Self::Sandbox,
        Self::Process,
        Self::Network,
        Self::Resource,
        Self::DeterministicGateFailure,
        Self::GateInfrastructureFailure,
        Self::ReviewDisagreement,
        Self::ReviewInvalidFinding,
        Self::ReviewUnresolvedBlocker,
        Self::ReviewOscillation,
        Self::Journal,
        Self::Artifact,
        Self::Projection,
        Self::Migration,
        Self::Recovery,
        Self::AuthorityTimeout,
        Self::AuthorityDenied,
        Self::SchedulerStarvation,
        Self::SchedulerCancellation,
        Self::SchedulerDependencyFailure,
        Self::EvolutionContamination,
        Self::EvolutionAttributionUncertainty,
        Self::EvolutionStatisticalRejection,
        Self::EvolutionPromotionDenial,
    ];

    /// Returns the stable append-only schema-v1 tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        self as u16
    }

    /// Strictly decodes one schema-v1 tag.
    ///
    /// # Errors
    ///
    /// Unknown tags reject; no `Unknown` substitute exists.
    pub fn from_tag(tag: u16) -> Result<Self, DebuggerError> {
        match tag {
            101 => Ok(Self::SpecificationAmbiguity),
            102 => Ok(Self::SpecificationConflict),
            103 => Ok(Self::SpecificationUnachievable),
            201 => Ok(Self::ContextSelection),
            202 => Ok(Self::ContextCompaction),
            203 => Ok(Self::ContextProvenance),
            301 => Ok(Self::ModelReasoning),
            302 => Ok(Self::ModelMalformedOutput),
            303 => Ok(Self::ModelRefusal),
            304 => Ok(Self::ModelCompletion),
            401 => Ok(Self::ProviderAuthentication),
            402 => Ok(Self::ProviderQuota),
            403 => Ok(Self::ProviderRateLimit),
            404 => Ok(Self::ProviderTransport),
            405 => Ok(Self::ProviderProtocol),
            406 => Ok(Self::ProviderAccounting),
            501 => Ok(Self::ToolSchema),
            502 => Ok(Self::ToolRouting),
            503 => Ok(Self::ToolAuthorization),
            504 => Ok(Self::ToolExecution),
            505 => Ok(Self::ToolResultNormalization),
            601 => Ok(Self::Workspace),
            602 => Ok(Self::Patch),
            603 => Ok(Self::Git),
            604 => Ok(Self::PathConflict),
            701 => Ok(Self::Sandbox),
            702 => Ok(Self::Process),
            703 => Ok(Self::Network),
            704 => Ok(Self::Resource),
            801 => Ok(Self::DeterministicGateFailure),
            802 => Ok(Self::GateInfrastructureFailure),
            901 => Ok(Self::ReviewDisagreement),
            902 => Ok(Self::ReviewInvalidFinding),
            903 => Ok(Self::ReviewUnresolvedBlocker),
            904 => Ok(Self::ReviewOscillation),
            1001 => Ok(Self::Journal),
            1002 => Ok(Self::Artifact),
            1003 => Ok(Self::Projection),
            1004 => Ok(Self::Migration),
            1005 => Ok(Self::Recovery),
            1101 => Ok(Self::AuthorityTimeout),
            1102 => Ok(Self::AuthorityDenied),
            1201 => Ok(Self::SchedulerStarvation),
            1202 => Ok(Self::SchedulerCancellation),
            1203 => Ok(Self::SchedulerDependencyFailure),
            1301 => Ok(Self::EvolutionContamination),
            1302 => Ok(Self::EvolutionAttributionUncertainty),
            1303 => Ok(Self::EvolutionStatisticalRejection),
            1304 => Ok(Self::EvolutionPromotionDenial),
            _ => Err(DebuggerError::new(
                DebuggerErrorKind::Taxonomy,
                DebuggerOperation::AnalyzeCauses,
                DebuggerRecovery::CorrectInput,
                "unknown schema-v1 failure-category tag",
            )),
        }
    }

    /// Returns the broad category family from the numeric schema partition.
    #[must_use]
    pub const fn family(self) -> u16 {
        self.tag() / 100
    }
}
