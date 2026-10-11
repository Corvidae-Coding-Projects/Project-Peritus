//! Verified generation-bound exclusive mutation leases for Peritus.
//!
//! Reducers are deterministic and effect-free. Successful values are logical state plans and raw
//! observations remain unprivileged; this crate never claims durable commit or effect authority.

mod binding;
mod claim;
mod command;
mod failure;
mod model;
mod port;
mod proofs;
mod reconcile;
mod scope;
mod state;
mod transition;
pub use binding::{
    LeaseCommandBinding, LeaseCommandBindingKind, LeasePermissionBinding, LeaseUseCommandBinding,
};
pub use claim::LeaseClaim;
pub use command::{
    AcquireLease, ExpireLease, FenceClockDiscontinuity, FenceHolderLoss, MintLease, ReconcileLease,
    ReleaseLease, RenewLease, RevokeLease, UseLease,
};
pub use failure::{
    LeaseError, LeasePhase, LeaseTransitionFailure, LeaseUseFailure, PolicyIntersectionDimension,
    ReconciliationDimension, RecoveryClass, ScopeDimension,
};
pub use port::{
    LeaseCasExpectation, LeaseCasObservation, LeaseCasPort, LeaseCasRequest, LeaseCasResolution,
    LeasePortFailure, ObservedLeaseState, ProtocolViolation, ValidatedAppliedLeaseClaim,
};
pub use reconcile::{
    FenceCause, HolderLossEvidence, HolderQuiescenceEvidence, ReconciliationCorrelation,
    ReconciliationDisposition, ReconciliationObservation,
};
pub use scope::{LeaseDuration, LeaseHolder, LeaseScope};
pub use state::{
    ActiveLeaseView, LeaseAggregate, QuarantinedLeaseView, ReconciliationView, RetirementReason,
};
pub use transition::{
    LeaseTransition, LeaseTransitionKind, LeaseTransitionOutcome, LeaseTransitionRecord,
    LeaseUseOutcome, LeaseUseTransition,
};
