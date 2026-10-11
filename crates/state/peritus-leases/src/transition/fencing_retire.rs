//! Terminal fencing transition when version or generation space is exhausted.

use super::fencing_apply::FencePlan;
use super::{LeaseAggregate, LeaseState, LeaseTransitionKind, TransitionPlan, transition};
use crate::{LeaseTransitionOutcome, RetirementReason};

pub(super) fn retire(
    before: LeaseAggregate,
    version: peritus_types::RevisionNumber,
    generation: peritus_types::Generation,
    reason: RetirementReason,
    plan: FencePlan,
) -> LeaseTransitionOutcome {
    let (command_id, time_advance, _observed_at, _cause, _normal_kind, binding) = plan.into_parts();
    let accepted = match transition(
        before,
        TransitionPlan::new(
            command_id,
            version,
            generation,
            time_advance,
            LeaseState::Retired(reason),
            LeaseTransitionKind::Retired(reason),
            binding,
        ),
    ) {
        LeaseTransitionOutcome::Accepted(accepted) => accepted,
        LeaseTransitionOutcome::Rejected(failure) => {
            return LeaseTransitionOutcome::Rejected(failure);
        }
    };
    LeaseTransitionOutcome::Accepted(accepted)
}
