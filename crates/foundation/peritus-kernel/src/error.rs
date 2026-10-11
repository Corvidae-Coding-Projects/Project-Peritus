//! Stable reducer failures.

#[doc = " Lifecycle entity named by a reducer failure."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LifecycleEntity {
    #[doc = " Session state."]
    Session,
    #[doc = " Run state."]
    Run,
    #[doc = " Attempt state."]
    Attempt,
    #[doc = " Turn state."]
    Turn,
    #[doc = " Action state."]
    Action,
    #[doc = " Review-cycle state."]
    Review,
    #[doc = " Waiver state."]
    Waiver,
    #[doc = " Acceptance state."]
    Acceptance,
}
#[doc = " External verified fact required by a command."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AuthorityInputKind {
    #[doc = " Root run budget snapshot."]
    RunBudget,
    #[doc = " Child attempt budget snapshot."]
    AttemptBudget,
    #[doc = " Parent budget snapshot used for child containment."]
    ParentBudget,
    #[doc = " Exact action-bound B1 capability use."]
    CapabilityUse,
    #[doc = " Exact-revision B2 acceptance evidence."]
    AcceptanceEvidence,
}
#[doc = " Stable machine-actionable reducer failure class."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KernelErrorKind {
    #[doc = " The envelope names another revision."]
    RevisionMismatch,
    #[doc = " The supplied contract differs from the aggregate\'s immutable binding."]
    ContractMismatch,
    #[doc = " The expected causal predecessor is not the current head."]
    CausalHeadMismatch,
    #[doc = " A command identity was already accepted."]
    DuplicateCommand,
    #[doc = " An event identity was already accepted."]
    DuplicateEvent,
    #[doc = " A named lifecycle entity does not exist."]
    MissingEntity,
    #[doc = " A child identity already exists."]
    DuplicateEntity,
    #[doc = " The named parent does not own the child."]
    ParentMismatch,
    #[doc = " The command is not legal from the current phase."]
    IllegalPhase,
    #[doc = " A required verified input was absent."]
    MissingAuthorityInput,
    #[doc = " A supplied verified input does not bind the requested state."]
    AuthorityMismatch,
    #[doc = " A budget account is not open for new work."]
    BudgetUnavailable,
    #[doc = " Child limits exceed the exact current parent availability."]
    BudgetExceeded,
    #[doc = " A live child prevents the requested parent transition."]
    LiveChild,
    #[doc = " Event sequence cannot advance without overflow."]
    SequenceOverflow,
    #[doc = " The proposed next aggregate violates a kernel invariant."]
    InvalidAggregate,
}
#[doc = " Typed deterministic lifecycle reducer failure."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct KernelError {
    kind: KernelErrorKind,
    entity: Option<LifecycleEntity>,
    authority: Option<AuthorityInputKind>,
}
impl KernelError {
    pub(crate) const fn new(kind: KernelErrorKind) -> Self {
        Self { kind, entity: None, authority: None }
    }
    pub(crate) const fn entity(kind: KernelErrorKind, entity: LifecycleEntity) -> Self {
        Self { kind, entity: Some(entity), authority: None }
    }
    pub(crate) const fn authority(kind: KernelErrorKind, authority: AuthorityInputKind) -> Self {
        Self { kind, entity: None, authority: Some(authority) }
    }
    #[doc = " Returns the stable failure class."]
    #[must_use]
    pub const fn kind(self) -> KernelErrorKind {
        self.kind
    }
    #[doc = " Returns the affected lifecycle entity, when applicable."]
    #[must_use]
    pub const fn affected_entity(self) -> Option<LifecycleEntity> {
        self.entity
    }
    #[doc = " Returns the required authority input, when applicable."]
    #[must_use]
    pub const fn authority_input(self) -> Option<AuthorityInputKind> {
        self.authority
    }
}
