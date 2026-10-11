//! Stable security roles and compiled role-separation decisions.

#[doc = " Stable security identity used by capability and approval policy."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ActorRole {
    #[doc = " Produces candidate workspace changes."]
    Writer,
    #[doc = " Resolves current-review findings through scoped changes."]
    Fixer,
    #[doc = " Performs fresh-context read-only adversarial review."]
    Reviewer,
    #[doc = " Evaluates candidates against isolated definitions and datasets."]
    Evaluator,
    #[doc = " Executes deterministic quality gates without accepting results."]
    GateRunner,
    #[doc = " Coordinates verified transitions without raw effect authority."]
    Orchestrator,
    #[doc = " Proposes and evaluates harness evolution candidates."]
    EvolutionAgent,
    #[doc = " Represents an authenticated human authority."]
    HumanAuthority,
    #[doc = " Runs daemon-owned control-plane work."]
    DaemonService,
    #[doc = " Executes a previously authorized provider or tool plan."]
    ProviderToolWorker,
    #[doc = " Represents an untrusted extension boundary."]
    Plugin,
}
impl ActorRole {
    pub(crate) const fn canonical_rank(self) -> u8 {
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
    #[doc = " Returns whether this role may receive the operation class under compiled invariants."]
    #[must_use]
    pub const fn permits_operation(self, operation: OperationClass) -> bool {
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
            Self::Evaluator | Self::GateRunner => {
                matches!(operation, OperationClass::Inspection | OperationClass::Execution)
            }
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
#[doc = " Stable operation category used by non-configurable role separation."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OperationClass {
    #[doc = " Reads state without changing a workspace or authoritative record."]
    Inspection,
    #[doc = " Mutates a workspace revision."]
    WorkspaceMutation,
    #[doc = " Executes a bounded process or deterministic gate."]
    Execution,
    #[doc = " Accesses an explicitly scoped network destination."]
    Network,
    #[doc = " Changes dependencies or the execution environment."]
    DependencyEnvironment,
    #[doc = " Rewrites repository history or protected references."]
    RepositoryHistoryMutation,
    #[doc = " Uses an injected secret without exposing its value."]
    SecretUse,
    #[doc = " Causes an externally visible side effect."]
    ExternalSideEffect,
    #[doc = " Accepts a run or candidate revision."]
    Acceptance,
    #[doc = " Waives an otherwise required finding or gate."]
    Waiver,
    #[doc = " Creates or activates a protected policy amendment."]
    PolicyAmendment,
    #[doc = " Promotes or rolls back a production harness."]
    HarnessPromotion,
    #[doc = " Exercises a decision reserved to authenticated human authority."]
    HumanAuthority,
    #[doc = " Invokes a raw effect adapter; only a provider/tool worker may receive this class."]
    RawEffect,
}
