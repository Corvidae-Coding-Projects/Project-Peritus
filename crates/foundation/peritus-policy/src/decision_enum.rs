//! Exhaustive move-only public policy decision.

// Verus lowers tuple variants to synthetic constructor methods without carrying their variant
// documentation. This module contains only the fully documented enum and scopes the workaround
// to those generated methods.
#![allow(missing_docs)]

use crate::{AuthorizationDenial, CapabilityIssuancePlan, EscalationChallenge};
use vstd::prelude::*;

verus! {

/// Exact exhaustive result of evaluating one complete authorization request.
///
/// Every authority-bearing payload remains move-only. Exhaustive matching makes the security
/// outcome explicit without permitting callers to construct an unchecked payload.
#[derive(Debug, Eq, PartialEq)]
pub enum PolicyDecision {
    /// The whole effective scope may proceed to logical capability issuance.
    Authorized(CapabilityIssuancePlan),
    /// The whole effective scope requires one conjunction-satisfying approval.
    ApprovalRequired(EscalationChallenge),
    /// The whole request was denied; no authorized subset exists.
    Denied(AuthorizationDenial),
}

} // verus!
