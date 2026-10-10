//! Stable whole-request policy denial reasons.

#[doc = " Stable reason that policy denied a complete request."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AuthorizationDenialReason {
    #[doc = " The request named a policy other than the evaluated immutable policy."]
    PolicyMismatch,
    #[doc = " The request exceeded the policy\'s complete parent boundary."]
    OutsideAuthorityBoundary,
    #[doc = " An exact capability name had no authenticated operation descriptor."]
    UnknownOperation,
    #[doc = " A compiled actor-role restriction rejected an operation class."]
    RoleSeparation,
    #[doc = " One or more exact permission pairs had no applicable ceiling grant."]
    IncompleteCeilingCoverage,
    #[doc = " An immutable ceiling denial matched at least one requested pair."]
    ImmutableDeny,
    #[doc = " A lower restriction-layer denial matched at least one requested pair."]
    RestrictionDeny,
    #[doc = " Applicable time constraints had an empty intersection."]
    EmptyConstraintIntersection,
    #[doc = " Approval role or validity constraints could not all be satisfied."]
    ApprovalConstraintConflict,
    #[doc = " Evaluation occurred before the effective validity window."]
    NotYetValid,
    #[doc = " Evaluation occurred at or after the effective expiry."]
    Expired,
}
