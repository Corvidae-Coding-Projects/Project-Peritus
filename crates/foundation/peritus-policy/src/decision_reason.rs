//! Stable whole-request policy denial reasons.

use vstd::prelude::*;

verus! {

/// Stable reason that policy denied a complete request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AuthorizationDenialReason {
    /// The request named a policy other than the evaluated immutable policy.
    PolicyMismatch,
    /// The request exceeded the policy's complete parent boundary.
    OutsideAuthorityBoundary,
    /// An exact capability name had no authenticated operation descriptor.
    UnknownOperation,
    /// A compiled actor-role restriction rejected an operation class.
    RoleSeparation,
    /// One or more exact permission pairs had no applicable ceiling grant.
    IncompleteCeilingCoverage,
    /// An immutable ceiling denial matched at least one requested pair.
    ImmutableDeny,
    /// A lower restriction-layer denial matched at least one requested pair.
    RestrictionDeny,
    /// Applicable time constraints had an empty intersection.
    EmptyConstraintIntersection,
    /// Approval role or validity constraints could not all be satisfied.
    ApprovalConstraintConflict,
    /// Evaluation occurred before the effective validity window.
    NotYetValid,
    /// Evaluation occurred at or after the effective expiry.
    Expired,
}

} // verus!
