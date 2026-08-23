//! Stable security roles and compiled role-separation decisions.

#[cfg(verus_only)]
use crate::model;
use vstd::prelude::*;

verus! {

/// Stable security identity used by capability and approval policy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ActorRole {
    /// Produces candidate workspace changes.
    Writer,
    /// Resolves current-review findings through scoped changes.
    Fixer,
    /// Performs fresh-context read-only adversarial review.
    Reviewer,
    /// Evaluates candidates against isolated definitions and datasets.
    Evaluator,
    /// Executes deterministic quality gates without accepting results.
    GateRunner,
    /// Coordinates verified transitions without raw effect authority.
    Orchestrator,
    /// Proposes and evaluates harness evolution candidates.
    EvolutionAgent,
    /// Represents an authenticated human authority.
    HumanAuthority,
    /// Runs daemon-owned control-plane work.
    DaemonService,
    /// Executes a previously authorized provider or tool plan.
    ProviderToolWorker,
    /// Represents an untrusted extension boundary.
    Plugin,
}

impl ActorRole {
    /// Returns the stable canonical role rank used by executable specifications.
    pub open spec fn spec_rank(self) -> int {
        match self {
            Self::Writer => 0,
            Self::Fixer => 1,
            Self::Reviewer => 2,
            Self::Evaluator => 3,
            Self::GateRunner => 4,
            Self::Orchestrator => 5,
            Self::EvolutionAgent => 6,
            Self::HumanAuthority => 7,
            Self::DaemonService => 8,
            Self::ProviderToolWorker => 9,
            Self::Plugin => 10,
        }
    }

    pub(crate) const fn canonical_rank(self) -> (rank: u8)
        ensures rank as int == self.spec_rank(),
    {
        match self {
            Self::Writer => 0,
            Self::Fixer => 1,
            Self::Reviewer => 2,
            Self::Evaluator => 3,
            Self::GateRunner => 4,
            Self::Orchestrator => 5,
            Self::EvolutionAgent => 6,
            Self::HumanAuthority => 7,
            Self::DaemonService => 8,
            Self::ProviderToolWorker => 9,
            Self::Plugin => 10,
        }
    }

    /// Returns whether this role may receive the operation class under compiled invariants.
    #[must_use]
    pub const fn permits_operation(self, operation: OperationClass) -> (result: bool)
        ensures result == model::role_permits(self, operation),
    {
        match self {
            Self::Writer | Self::Fixer => matches!(
                operation,
                OperationClass::Inspection
                    | OperationClass::WorkspaceMutation
                    | OperationClass::Execution
                    | OperationClass::Network
                    | OperationClass::DependencyEnvironment
                    | OperationClass::RepositoryHistoryMutation
                    | OperationClass::SecretUse
                    | OperationClass::ExternalSideEffect
            ),
            Self::Reviewer | Self::Plugin => {
                matches!(operation, OperationClass::Inspection)
            }
            Self::Evaluator | Self::GateRunner => matches!(
                operation,
                OperationClass::Inspection | OperationClass::Execution
            ),
            Self::Orchestrator => matches!(
                operation,
                OperationClass::Inspection
                    | OperationClass::Execution
                    | OperationClass::Acceptance
                    | OperationClass::PolicyAmendment
                    | OperationClass::HarnessPromotion
            ),
            Self::EvolutionAgent => matches!(
                operation,
                OperationClass::Inspection
                    | OperationClass::WorkspaceMutation
                    | OperationClass::Execution
                    | OperationClass::Network
                    | OperationClass::DependencyEnvironment
            ),
            Self::HumanAuthority => matches!(
                operation,
                OperationClass::Inspection
                    | OperationClass::Acceptance
                    | OperationClass::Waiver
                    | OperationClass::PolicyAmendment
                    | OperationClass::HarnessPromotion
                    | OperationClass::HumanAuthority
            ),
            Self::DaemonService => matches!(
                operation,
                OperationClass::Inspection
                    | OperationClass::Execution
                    | OperationClass::Network
                    | OperationClass::SecretUse
                    | OperationClass::ExternalSideEffect
                    | OperationClass::Acceptance
                    | OperationClass::PolicyAmendment
                    | OperationClass::HarnessPromotion
            ),
            Self::ProviderToolWorker => matches!(
                operation,
                OperationClass::Inspection
                    | OperationClass::WorkspaceMutation
                    | OperationClass::Execution
                    | OperationClass::Network
                    | OperationClass::DependencyEnvironment
                    | OperationClass::RepositoryHistoryMutation
                    | OperationClass::SecretUse
                    | OperationClass::ExternalSideEffect
                    | OperationClass::RawEffect
            ),
        }
    }
}

/// Stable operation category used by non-configurable role separation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OperationClass {
    /// Reads state without changing a workspace or authoritative record.
    Inspection,
    /// Mutates a workspace revision.
    WorkspaceMutation,
    /// Executes a bounded process or deterministic gate.
    Execution,
    /// Accesses an explicitly scoped network destination.
    Network,
    /// Changes dependencies or the execution environment.
    DependencyEnvironment,
    /// Rewrites repository history or protected references.
    RepositoryHistoryMutation,
    /// Uses an injected secret without exposing its value.
    SecretUse,
    /// Causes an externally visible side effect.
    ExternalSideEffect,
    /// Accepts a run or candidate revision.
    Acceptance,
    /// Waives an otherwise required finding or gate.
    Waiver,
    /// Creates or activates a protected policy amendment.
    PolicyAmendment,
    /// Promotes or rolls back a production harness.
    HarnessPromotion,
    /// Exercises a decision reserved to authenticated human authority.
    HumanAuthority,
    /// Invokes a raw effect adapter; only a provider/tool worker may receive this class.
    RawEffect,
}

} // verus!
