//! Core lifecycle safety proofs.

use crate::{AcceptancePhase, KernelEventKind, RunPhase};
use crate::model::{KernelModelState, accepted_state_is_complete, legal_model_step};
use vstd::prelude::*;

verus! {

/// INV-002: every admitted model step advances the event sequence exactly once.
pub(crate) proof fn legal_step_advances_sequence_once(
    before: KernelModelState,
    event: KernelEventKind,
    after: KernelModelState,
)
    requires legal_model_step(before, event, after),
    ensures after.sequence == before.sequence + 1,
{}

/// INV-005: failure, cancellation, exhaustion, pause, and needs-changes events cannot accept.
pub(crate) proof fn non_success_step_cannot_accept(
    before: KernelModelState,
    event: KernelEventKind,
    after: KernelModelState,
)
    requires
        legal_model_step(before, event, after),
        crate::model::is_non_success_event(event),
    ensures
        after.run != Some(RunPhase::Accepted),
        after.acceptance != Some(AcceptancePhase::Accepted),
{
    if after.acceptance == Some(AcceptancePhase::Accepted) {
        assert(!accepted_state_is_complete(after));
    }
}

/// INV-004/005 bridge: accepted state is reachable only through acceptance evaluation.
pub(crate) proof fn accepted_step_requires_acceptance_event(
    before: KernelModelState,
    event: KernelEventKind,
    after: KernelModelState,
)
    requires
        legal_model_step(before, event, after),
        after.run == Some(RunPhase::Accepted),
    ensures
        event == KernelEventKind::AcceptanceAccepted,
        before.run == Some(RunPhase::Reviewing),
        before.acceptance == Some(AcceptancePhase::Evaluating),
        after.acceptance == Some(AcceptancePhase::Accepted),
{}

} // verus!
