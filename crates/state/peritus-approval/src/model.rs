//! Closed total approval transition model used by refinement proofs.

// Verus lowers documented payload variants to synthetic methods without carrying documentation.
// This module's public items are fully documented; this scopes the workaround to those artifacts.
#![allow(missing_docs)]

use vstd::prelude::*;

verus! {

/// Mathematical state tracking semantic resolution and one-time use counts.
pub struct ApprovalModelState {
    /// Public lifecycle phase.
    pub phase: crate::ApprovalPhase,
    /// Exact semantic resolution digest, when resolved.
    pub decision: Option<crate::ApprovalDecisionDigest>,
    /// Number of non-idempotent semantic resolutions.
    pub resolution_count: nat,
    /// Number of approve-once consumptions.
    pub use_count: nat,
}

/// Total-model input; illegal steps leave state unchanged.
pub enum ApprovalModelStep {
    /// Resolve pending state as approve once.
    ResolveApproveOnce(crate::ApprovalDecisionDigest),
    /// Resolve pending state as amendment authorization.
    ResolveAmendment(crate::ApprovalDecisionDigest),
    /// Resolve pending state as denial.
    ResolveDeny(crate::ApprovalDecisionDigest),
    /// Replay an exact already-resolved decision.
    Replay(crate::ApprovalDecisionDigest),
    /// Consume approve once.
    ConsumeOnce,
    /// Consume amendment authorization.
    ConsumeAmendment,
    /// Expire pending or unconsumed authority.
    Expire,
    /// Cancel pending state.
    Cancel,
}

/// Returns the exact pending-resolution model step for one authenticated choice.
pub open spec fn resolution_step(
    choice: crate::ApprovalChoice,
    digest: crate::ApprovalDecisionDigest,
) -> ApprovalModelStep {
    match choice {
        crate::ApprovalChoice::Deny => ApprovalModelStep::ResolveDeny(digest),
        crate::ApprovalChoice::ApproveOnce => ApprovalModelStep::ResolveApproveOnce(digest),
        crate::ApprovalChoice::Amend(_) => ApprovalModelStep::ResolveAmendment(digest),
    }
}

/// Returns the unique unresolved initial model state.
pub open spec fn initial() -> ApprovalModelState {
    ApprovalModelState {
        phase: crate::ApprovalPhase::Pending,
        decision: None,
        resolution_count: 0,
        use_count: 0,
    }
}

/// Applies one total closed-model step; illegal steps preserve the input state.
pub open spec fn next(
    state: ApprovalModelState,
    step: ApprovalModelStep,
) -> ApprovalModelState {
    match step {
        ApprovalModelStep::ResolveApproveOnce(digest)
            if state.phase == crate::ApprovalPhase::Pending => ApprovalModelState {
                phase: crate::ApprovalPhase::ApprovedOnce,
                decision: Some(digest),
                resolution_count: 1,
                use_count: 0,
            },
        ApprovalModelStep::ResolveAmendment(digest)
            if state.phase == crate::ApprovalPhase::Pending => ApprovalModelState {
                phase: crate::ApprovalPhase::AmendmentAuthorized,
                decision: Some(digest),
                resolution_count: 1,
                use_count: 0,
            },
        ApprovalModelStep::ResolveDeny(digest)
            if state.phase == crate::ApprovalPhase::Pending => ApprovalModelState {
                phase: crate::ApprovalPhase::Denied,
                decision: Some(digest),
                resolution_count: 1,
                use_count: 0,
            },
        ApprovalModelStep::ConsumeOnce
            if state.phase == crate::ApprovalPhase::ApprovedOnce => ApprovalModelState {
                phase: crate::ApprovalPhase::Consumed,
                decision: state.decision,
                resolution_count: state.resolution_count,
                use_count: 1,
            },
        ApprovalModelStep::ConsumeAmendment
            if state.phase == crate::ApprovalPhase::AmendmentAuthorized => ApprovalModelState {
                phase: crate::ApprovalPhase::Amended,
                decision: state.decision,
                resolution_count: state.resolution_count,
                use_count: 0,
            },
        ApprovalModelStep::Expire
            if state.phase == crate::ApprovalPhase::Pending
                || state.phase == crate::ApprovalPhase::ApprovedOnce
                || state.phase == crate::ApprovalPhase::AmendmentAuthorized => ApprovalModelState {
                phase: crate::ApprovalPhase::Expired,
                decision: state.decision,
                resolution_count: state.resolution_count,
                use_count: state.use_count,
            },
        ApprovalModelStep::Cancel
            if state.phase == crate::ApprovalPhase::Pending => ApprovalModelState {
                phase: crate::ApprovalPhase::Cancelled,
                decision: None,
                resolution_count: 0,
                use_count: 0,
            },
        ApprovalModelStep::Replay(_) => state,
        _ => state,
    }
}

/// Folds an exact step trace over one starting state.
pub open spec fn reachable(
    state: ApprovalModelState,
    trace: Seq<ApprovalModelStep>,
) -> ApprovalModelState
    decreases trace.len(),
{
    if trace.len() == 0 {
        state
    } else {
        let prefix = trace.subrange(0, trace.len() - 1);
        next(reachable(state, prefix), trace[trace.len() - 1])
    }
}

/// States INV-009: at most one resolution and at most one exact approve-once use.
pub open spec fn inv_009(state: ApprovalModelState) -> bool {
    state.resolution_count <= 1
        && state.use_count <= 1
        && state.use_count <= state.resolution_count
        && match state.phase {
            crate::ApprovalPhase::Pending | crate::ApprovalPhase::Cancelled => {
                state.resolution_count == 0 && state.use_count == 0 && state.decision.is_none()
            }
            crate::ApprovalPhase::ApprovedOnce
            | crate::ApprovalPhase::AmendmentAuthorized
            | crate::ApprovalPhase::Denied
            | crate::ApprovalPhase::Amended => {
                state.resolution_count == 1 && state.use_count == 0 && state.decision.is_some()
            }
            crate::ApprovalPhase::Consumed => {
                state.resolution_count == 1 && state.use_count == 1 && state.decision.is_some()
            }
            crate::ApprovalPhase::Expired => {
                state.use_count == 0
                    && ((state.resolution_count == 0 && state.decision.is_none())
                        || (state.resolution_count == 1 && state.decision.is_some()))
            }
        }
}

/// Returns whether a public lifecycle phase cannot accept a new semantic resolution.
pub open spec fn terminal(phase: crate::ApprovalPhase) -> bool {
    phase == crate::ApprovalPhase::Consumed
        || phase == crate::ApprovalPhase::Amended
        || phase == crate::ApprovalPhase::Denied
        || phase == crate::ApprovalPhase::Expired
        || phase == crate::ApprovalPhase::Cancelled
}

} // verus!
