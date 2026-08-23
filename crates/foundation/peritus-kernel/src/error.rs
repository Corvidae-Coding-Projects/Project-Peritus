//! Stable reducer failures.

use vstd::prelude::*;

verus! {

/// Lifecycle entity named by a reducer failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LifecycleEntity {
    /// Session state.
    Session,
    /// Run state.
    Run,
    /// Attempt state.
    Attempt,
    /// Turn state.
    Turn,
    /// Action state.
    Action,
    /// Review-cycle state.
    Review,
    /// Waiver state.
    Waiver,
    /// Acceptance state.
    Acceptance,
}

/// External verified fact required by a command.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AuthorityInputKind {
    /// Root run budget snapshot.
    RunBudget,
    /// Child attempt budget snapshot.
    AttemptBudget,
    /// Parent budget snapshot used for child containment.
    ParentBudget,
    /// Exact action-bound B1 capability use.
    CapabilityUse,
    /// Exact-revision B2 acceptance evidence.
    AcceptanceEvidence,
}

/// Stable machine-actionable reducer failure class.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KernelErrorKind {
    /// The envelope names another revision.
    RevisionMismatch,
    /// The supplied contract differs from the aggregate's immutable binding.
    ContractMismatch,
    /// The expected causal predecessor is not the current head.
    CausalHeadMismatch,
    /// A command identity was already accepted.
    DuplicateCommand,
    /// An event identity was already accepted.
    DuplicateEvent,
    /// A named lifecycle entity does not exist.
    MissingEntity,
    /// A child identity already exists.
    DuplicateEntity,
    /// The named parent does not own the child.
    ParentMismatch,
    /// The command is not legal from the current phase.
    IllegalPhase,
    /// A required verified input was absent.
    MissingAuthorityInput,
    /// A supplied verified input does not bind the requested state.
    AuthorityMismatch,
    /// A budget account is not open for new work.
    BudgetUnavailable,
    /// Child limits exceed the exact current parent availability.
    BudgetExceeded,
    /// A live child prevents the requested parent transition.
    LiveChild,
    /// Event sequence cannot advance without overflow.
    SequenceOverflow,
    /// The proposed next aggregate violates a kernel invariant.
    InvalidAggregate,
}

/// Typed deterministic lifecycle reducer failure.
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

    /// Returns the stable failure class.
    #[must_use]
    pub const fn kind(self) -> KernelErrorKind { self.kind }
    /// Returns the affected lifecycle entity, when applicable.
    #[must_use]
    pub const fn affected_entity(self) -> Option<LifecycleEntity> { self.entity }
    /// Returns the required authority input, when applicable.
    #[must_use]
    pub const fn authority_input(self) -> Option<AuthorityInputKind> { self.authority }
}

} // verus!
