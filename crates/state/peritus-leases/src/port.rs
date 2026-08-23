//! Durable compare-and-swap request and untrusted observation contracts.
#![allow(
    missing_docs,
    reason = "pinned Cargo-Verus synthesizes undocumented accessors for documented payload variants"
)]

use crate::{LeaseAggregate, LeaseTransitionRecord, RecoveryClass};
use peritus_types::{CommandId, RevisionNumber, WorkspaceId};
use vstd::prelude::*;

verus! {

mod contracts;
mod equality;

/// Exact expected aggregate state for one compare-and-swap plan.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LeaseCasExpectation {
    /// The workspace aggregate must not exist.
    Absent,
    /// The workspace aggregate must have this exact version.
    Version(RevisionNumber),
}

/// Exact unprivileged durable-CAS plan produced only from an accepted lease transition.
#[derive(Debug, Eq, Hash, PartialEq)]
pub struct LeaseCasRequest {
    workspace_id: WorkspaceId,
    expected: LeaseCasExpectation,
    command_id: CommandId,
    planned: LeaseAggregate,
    record: LeaseTransitionRecord,
}

impl LeaseCasRequest {
    /// Returns the exact aggregate key.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId { self.workspace_id }

    /// Returns the exact absence/version precondition.
    #[must_use]
    pub const fn expected(&self) -> LeaseCasExpectation { self.expected }

    /// Returns the idempotency identity that must also be used for resolution.
    #[must_use]
    pub const fn command_id(&self) -> CommandId { self.command_id }

    /// Borrows the exact planned successor snapshot.
    #[must_use]
    pub const fn planned(&self) -> &LeaseAggregate { &self.planned }

    /// Returns the exact typed transition record.
    #[must_use]
    pub const fn record(&self) -> &LeaseTransitionRecord { &self.record }

}

/// Aggregate state reported by a conflict observation.
#[derive(Debug, Eq, Hash, PartialEq)]
pub enum ObservedLeaseState {
    /// The aggregate was absent at observation time.
    Absent,
    /// The adapter returned one checked, replayed logical snapshot.
    Present(Box<LeaseAggregate>),
}

/// Bounded untrusted observation returned by a state-owner adapter.
#[derive(Debug, Eq, Hash, PartialEq)]
pub enum LeaseCasObservation {
    /// The adapter claims the exact submitted request applied.
    ///
    /// This carries identity only: the caller retains the move-only request and its complete plan.
    /// The claim is not a durable receipt and cannot authorize an effect.
    ClaimedApplied {
        /// Echoed aggregate key.
        workspace_id: WorkspaceId,
        /// Echoed idempotency identity.
        command_id: CommandId,
    },
    /// The exact precondition did not match current state.
    Conflict {
        /// Echoed aggregate key.
        workspace_id: WorkspaceId,
        /// Echoed idempotency identity.
        command_id: CommandId,
        /// Current unprivileged state observation.
        observed: ObservedLeaseState,
    },
    /// The adapter can prove this command identity did not apply.
    DefinitelyNotApplied {
        /// Echoed aggregate key.
        workspace_id: WorkspaceId,
        /// Echoed idempotency identity.
        command_id: CommandId,
    },
    /// The outcome must be resolved under the same command identity.
    Indeterminate {
        /// Echoed aggregate key.
        workspace_id: WorkspaceId,
        /// Echoed idempotency identity.
        command_id: CommandId,
    },
    /// The adapter detected a bounded protocol violation.
    ProtocolInvalid {
        /// Echoed aggregate key.
        workspace_id: WorkspaceId,
        /// Echoed idempotency identity.
        command_id: CommandId,
        /// Claimed protocol problem.
        violation: ProtocolViolation,
    },
}

/// Exact protocol-field violation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProtocolViolation {
    /// Key or command identity did not match the submitted request.
    IdentityMismatch,
    /// Authoritative stored fields did not exactly match the submitted plan.
    AuthoritativePlanMismatch,
    /// Adapter data could not be reconstructed as a checked lease snapshot.
    InvalidSnapshot,
    /// Adapter returned a response outside the bounded contract.
    MalformedObservation,
}

impl ProtocolViolation {
    /// Returns the stable subsystem diagnostic code for the exact malformed observation.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::IdentityMismatch => "PERITUS-LEASE-CAS-001",
            Self::AuthoritativePlanMismatch => "PERITUS-LEASE-CAS-002",
            Self::InvalidSnapshot => "PERITUS-LEASE-CAS-003",
            Self::MalformedObservation => "PERITUS-LEASE-CAS-004",
        }
    }

    /// Returns the recovery class for the exact malformed observation.
    #[must_use]
    pub const fn recovery(self) -> RecoveryClass {
        match self {
            Self::InvalidSnapshot => RecoveryClass::Terminal,
            Self::IdentityMismatch
            | Self::AuthoritativePlanMismatch
            | Self::MalformedObservation => RecoveryClass::CallerCorrectable,
        }
    }
}

/// Identity-matching applied claim. This remains unprivileged and move-only.
///
/// It cannot be duplicated:
///
/// ```compile_fail
/// use peritus_leases::ValidatedAppliedLeaseClaim;
/// fn require_clone<T: Clone>() {}
/// require_clone::<ValidatedAppliedLeaseClaim>();
/// ```
///
/// It is not policy-use authority:
///
/// ```compile_fail
/// use peritus_leases::ValidatedAppliedLeaseClaim;
/// fn authorize_effect(_: peritus_policy::CapabilityUseTransition) {
///     panic!("compile-fail example is never executed");
/// }
/// fn misuse(claim: ValidatedAppliedLeaseClaim) {
///     authorize_effect(claim);
/// }
/// ```
#[derive(Debug, Eq, Hash, PartialEq)]
pub struct ValidatedAppliedLeaseClaim {
    workspace_id: WorkspaceId,
    command_id: CommandId,
}

impl ValidatedAppliedLeaseClaim {
    /// Returns the exactly matching aggregate key.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId { self.workspace_id }

    /// Returns the exactly matching idempotency identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId { self.command_id }
}

/// Fail-closed interpretation of one observation against one request.
#[derive(Debug, Eq, Hash, PartialEq)]
pub enum LeaseCasResolution {
    /// The raw claim matches the request identity; durable commit and stored-plan equality remain
    /// unproved.
    ClaimedApplied(ValidatedAppliedLeaseClaim),
    /// The caller must reobserve, reauthorize, and replan.
    Conflict(ObservedLeaseState),
    /// The same command did not apply and may be replanned only after current-state observation.
    DefinitelyNotApplied,
    /// Resolve this exact command identity before any retry.
    Indeterminate,
    /// Reject the adapter response.
    ProtocolInvalid(ProtocolViolation),
}

/// Typed state-owner port failure before a bounded observation exists.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LeasePortFailure {
    /// The state owner was unavailable.
    Unavailable,
    /// The outcome may be applied and must be resolved with the same command identity.
    Indeterminate,
    /// The state owner rejected an invalid bounded request.
    ProtocolViolation(ProtocolViolation),
}

impl LeasePortFailure {
    /// Returns the stable subsystem diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "PERITUS-LEASE-PORT-001",
            Self::Indeterminate => "PERITUS-LEASE-PORT-002",
            Self::ProtocolViolation(violation) => violation.code(),
        }
    }

    /// Returns the mandatory recovery class.
    #[must_use]
    pub const fn recovery(self) -> RecoveryClass {
        match self {
            Self::Unavailable => RecoveryClass::Reobserve,
            Self::Indeterminate => RecoveryClass::ResolveIndeterminate,
            Self::ProtocolViolation(violation) => violation.recovery(),
        }
    }
}

/// Contract implemented later by C0's journal-backed transactional state owner.
///
/// Implementations must not interpret a raw B1 value as a durable receipt or effect permit.
pub trait LeaseCasPort {
    /// Submits one exact logical plan.
    ///
    /// # Errors
    ///
    /// Returns a typed availability, indeterminacy, or bounded protocol failure. An
    /// indeterminate result must be resolved under the same command identity.
    fn compare_and_swap(
        &mut self,
        request: &LeaseCasRequest,
    ) -> Result<LeaseCasObservation, LeasePortFailure>;

    /// Resolves the same aggregate key and command identity after an indeterminate result.
    ///
    /// # Errors
    ///
    /// Returns a typed availability, continued indeterminacy, or bounded protocol failure.
    fn resolve_command(
        &mut self,
        workspace_id: WorkspaceId,
        command_id: CommandId,
    ) -> Result<LeaseCasObservation, LeasePortFailure>;
}

} // verus!
