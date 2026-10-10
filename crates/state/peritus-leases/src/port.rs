//! Durable compare-and-swap request and untrusted observation contracts.
#![allow(
    missing_docs,
    reason = "pinned Cargo-Verus synthesizes undocumented accessors for documented payload variants"
)]

use crate::{LeaseAggregate, LeaseTransitionRecord, RecoveryClass};
use peritus_types::{CommandId, RevisionNumber, WorkspaceId};

mod contracts;
mod equality;
#[doc = " Exact expected aggregate state for one compare-and-swap plan."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LeaseCasExpectation {
    #[doc = " The workspace aggregate must not exist."]
    Absent,
    #[doc = " The workspace aggregate must have this exact version."]
    Version(RevisionNumber),
}
#[doc = " Exact unprivileged durable-CAS plan produced only from an accepted lease transition."]
#[derive(Debug, Eq, Hash, PartialEq)]
pub struct LeaseCasRequest {
    workspace_id: WorkspaceId,
    expected: LeaseCasExpectation,
    command_id: CommandId,
    planned: LeaseAggregate,
    record: LeaseTransitionRecord,
}
impl LeaseCasRequest {
    #[doc = " Returns the exact aggregate key."]
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    #[doc = " Returns the exact absence/version precondition."]
    #[must_use]
    pub const fn expected(&self) -> LeaseCasExpectation {
        self.expected
    }
    #[doc = " Returns the idempotency identity that must also be used for resolution."]
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    #[doc = " Borrows the exact planned successor snapshot."]
    #[must_use]
    pub const fn planned(&self) -> &LeaseAggregate {
        &self.planned
    }
    #[doc = " Returns the exact typed transition record."]
    #[must_use]
    pub const fn record(&self) -> &LeaseTransitionRecord {
        &self.record
    }
}
#[doc = " Aggregate state reported by a conflict observation."]
#[derive(Debug, Eq, Hash, PartialEq)]
pub enum ObservedLeaseState {
    #[doc = " The aggregate was absent at observation time."]
    Absent,
    #[doc = " The adapter returned one checked, replayed logical snapshot."]
    Present(Box<LeaseAggregate>),
}
#[doc = " Bounded untrusted observation returned by a state-owner adapter."]
#[derive(Debug, Eq, Hash, PartialEq)]
pub enum LeaseCasObservation {
    #[doc = " The adapter claims the exact submitted request applied."]
    #[doc = ""]
    #[doc = " This carries identity only: the caller retains the move-only request and its complete plan."]
    #[doc = " The claim is not a durable receipt and cannot authorize an effect."]
    ClaimedApplied {
        #[doc = " Echoed aggregate key."]
        workspace_id: WorkspaceId,
        #[doc = " Echoed idempotency identity."]
        command_id: CommandId,
    },
    #[doc = " The exact precondition did not match current state."]
    Conflict {
        #[doc = " Echoed aggregate key."]
        workspace_id: WorkspaceId,
        #[doc = " Echoed idempotency identity."]
        command_id: CommandId,
        #[doc = " Current unprivileged state observation."]
        observed: ObservedLeaseState,
    },
    #[doc = " The adapter can prove this command identity did not apply."]
    DefinitelyNotApplied {
        #[doc = " Echoed aggregate key."]
        workspace_id: WorkspaceId,
        #[doc = " Echoed idempotency identity."]
        command_id: CommandId,
    },
    #[doc = " The outcome must be resolved under the same command identity."]
    Indeterminate {
        #[doc = " Echoed aggregate key."]
        workspace_id: WorkspaceId,
        #[doc = " Echoed idempotency identity."]
        command_id: CommandId,
    },
    #[doc = " The adapter detected a bounded protocol violation."]
    ProtocolInvalid {
        #[doc = " Echoed aggregate key."]
        workspace_id: WorkspaceId,
        #[doc = " Echoed idempotency identity."]
        command_id: CommandId,
        #[doc = " Claimed protocol problem."]
        violation: ProtocolViolation,
    },
}
#[doc = " Exact protocol-field violation."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProtocolViolation {
    #[doc = " Key or command identity did not match the submitted request."]
    IdentityMismatch,
    #[doc = " Authoritative stored fields did not exactly match the submitted plan."]
    AuthoritativePlanMismatch,
    #[doc = " Adapter data could not be reconstructed as a checked lease snapshot."]
    InvalidSnapshot,
    #[doc = " Adapter returned a response outside the bounded contract."]
    MalformedObservation,
}
impl ProtocolViolation {
    #[doc = " Returns the stable subsystem diagnostic code for the exact malformed observation."]
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::IdentityMismatch => "PERITUS-LEASE-CAS-001",
            Self::AuthoritativePlanMismatch => "PERITUS-LEASE-CAS-002",
            Self::InvalidSnapshot => "PERITUS-LEASE-CAS-003",
            Self::MalformedObservation => "PERITUS-LEASE-CAS-004",
        }
    }
    #[doc = " Returns the recovery class for the exact malformed observation."]
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
#[doc = " Identity-matching applied claim. This remains unprivileged and move-only."]
#[doc = ""]
#[doc = " It cannot be duplicated:"]
#[doc = ""]
#[doc = " ```compile_fail"]
#[doc = " use peritus_leases::ValidatedAppliedLeaseClaim;"]
#[doc = " fn require_clone<T: Clone>() {}"]
#[doc = " require_clone::<ValidatedAppliedLeaseClaim>();"]
#[doc = " ```"]
#[doc = ""]
#[doc = " It is not policy-use authority:"]
#[doc = ""]
#[doc = " ```compile_fail"]
#[doc = " use peritus_leases::ValidatedAppliedLeaseClaim;"]
#[doc = " fn authorize_effect(_: peritus_policy::CapabilityUseTransition) {"]
#[doc = "     panic!(\"compile-fail example is never executed\");"]
#[doc = " }"]
#[doc = " fn misuse(claim: ValidatedAppliedLeaseClaim) {"]
#[doc = "     authorize_effect(claim);"]
#[doc = " }"]
#[doc = " ```"]
#[derive(Debug, Eq, Hash, PartialEq)]
pub struct ValidatedAppliedLeaseClaim {
    workspace_id: WorkspaceId,
    command_id: CommandId,
}
impl ValidatedAppliedLeaseClaim {
    #[doc = " Returns the exactly matching aggregate key."]
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    #[doc = " Returns the exactly matching idempotency identity."]
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
}
#[doc = " Fail-closed interpretation of one observation against one request."]
#[derive(Debug, Eq, Hash, PartialEq)]
pub enum LeaseCasResolution {
    #[doc = " The raw claim matches the request identity; durable commit and stored-plan equality remain"]
    #[doc = " unproved."]
    ClaimedApplied(ValidatedAppliedLeaseClaim),
    #[doc = " The caller must reobserve, reauthorize, and replan."]
    Conflict(ObservedLeaseState),
    #[doc = " The same command did not apply and may be replanned only after current-state observation."]
    DefinitelyNotApplied,
    #[doc = " Resolve this exact command identity before any retry."]
    Indeterminate,
    #[doc = " Reject the adapter response."]
    ProtocolInvalid(ProtocolViolation),
}
#[doc = " Typed state-owner port failure before a bounded observation exists."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LeasePortFailure {
    #[doc = " The state owner was unavailable."]
    Unavailable,
    #[doc = " The outcome may be applied and must be resolved with the same command identity."]
    Indeterminate,
    #[doc = " The state owner rejected an invalid bounded request."]
    ProtocolViolation(ProtocolViolation),
}
impl LeasePortFailure {
    #[doc = " Returns the stable subsystem diagnostic code."]
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "PERITUS-LEASE-PORT-001",
            Self::Indeterminate => "PERITUS-LEASE-PORT-002",
            Self::ProtocolViolation(violation) => violation.code(),
        }
    }
    #[doc = " Returns the mandatory recovery class."]
    #[must_use]
    pub const fn recovery(self) -> RecoveryClass {
        match self {
            Self::Unavailable => RecoveryClass::Reobserve,
            Self::Indeterminate => RecoveryClass::ResolveIndeterminate,
            Self::ProtocolViolation(violation) => violation.recovery(),
        }
    }
}
#[doc = " Contract implemented later by C0\'s journal-backed transactional state owner."]
#[doc = ""]
#[doc = " Implementations must not interpret a raw B1 value as a durable receipt or effect permit."]
pub trait LeaseCasPort {
    #[doc = " Submits one exact logical plan."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed availability, indeterminacy, or bounded protocol failure. An"]
    #[doc = " indeterminate result must be resolved under the same command identity."]
    fn compare_and_swap(
        &mut self,
        request: &LeaseCasRequest,
    ) -> Result<LeaseCasObservation, LeasePortFailure>;
    #[doc = " Resolves the same aggregate key and command identity after an indeterminate result."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed availability, continued indeterminacy, or bounded protocol failure."]
    fn resolve_command(
        &mut self,
        workspace_id: WorkspaceId,
        command_id: CommandId,
    ) -> Result<LeaseCasObservation, LeasePortFailure>;
}
