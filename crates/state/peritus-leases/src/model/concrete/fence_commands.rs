//! Command-specific exact accepted fencing contracts.

use vstd::prelude::*;

verus! {

pub(crate) open spec fn exact_fence_acceptance(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command_id: peritus_types::CommandId,
    normal_kind: crate::LeaseTransitionKind,
    cause: Option<crate::FenceCause>,
    observed_at: peritus_policy::AuthorityInstant,
) -> bool {
    super::concrete_fence_decision(
        before,
        &transition.next,
        transition.record,
        command_id,
        normal_kind,
        cause,
    ) && super::concrete_fence_time_observed(
        before,
        &transition.next,
        observed_at,
        cause,
    )
}

pub(crate) open spec fn exact_release_acceptance(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::ReleaseLease,
) -> bool {
    super::concrete_claim_is_current(before, command.claim)
        && transition.record.binding.matches_release(command)
        && match command.spec_quiescence() {
            Some(evidence) => {
                super::concrete_claim_matches(evidence.spec_claim(), command.claim)
                    && exact_fence_acceptance(
                        before,
                        transition,
                        command.command_id,
                        crate::LeaseTransitionKind::ReleasedAvailable,
                        None,
                        command.observed_at,
                    )
            }
            None => exact_fence_acceptance(
                before,
                transition,
                command.command_id,
                crate::LeaseTransitionKind::ReleasedReconciling,
                Some(crate::FenceCause::ReleasedWithoutQuiescence),
                command.observed_at,
            ),
        }
}

pub closed spec fn concrete_release_transition(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::ReleaseLease,
) -> bool {
    exact_release_acceptance(before, transition, command)
}

pub(crate) proof fn establish_release_transition(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::ReleaseLease,
)
    requires exact_release_acceptance(before, transition, command),
    ensures concrete_release_transition(before, transition, command),
{
}

pub(crate) open spec fn exact_expire_acceptance(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::ExpireLease,
) -> bool {
    transition.record.binding.matches_expire(command)
        && match before.state {
            crate::state::LeaseState::Active(active) => {
                command.observed_at.spec_epoch() == active.expires_at.spec_epoch()
                    && command.observed_at.spec_tick_millis()
                        >= active.expires_at.spec_tick_millis()
            }
            _ => false,
        }
        && exact_fence_acceptance(
            before,
            transition,
            command.command_id,
            crate::LeaseTransitionKind::Expired,
            Some(crate::FenceCause::Expired),
            command.observed_at,
        )
}

pub closed spec fn concrete_expire_transition(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::ExpireLease,
) -> bool {
    exact_expire_acceptance(before, transition, command)
}

pub(crate) proof fn establish_expire_transition(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::ExpireLease,
)
    requires exact_expire_acceptance(before, transition, command),
    ensures concrete_expire_transition(before, transition, command),
{
}

pub(crate) open spec fn exact_holder_loss_acceptance(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::FenceHolderLoss,
) -> bool {
    super::concrete_claim_is_current(before, command.evidence.spec_claim())
        && transition.record.binding.matches_holder_loss(command)
        && exact_fence_acceptance(
            before,
            transition,
            command.command_id,
            crate::LeaseTransitionKind::HolderLost,
            Some(crate::FenceCause::HolderLost),
            command.observed_at,
        )
}

pub closed spec fn concrete_holder_loss_transition(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::FenceHolderLoss,
) -> bool {
    exact_holder_loss_acceptance(before, transition, command)
}

pub(crate) proof fn establish_holder_loss_transition(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::FenceHolderLoss,
)
    requires exact_holder_loss_acceptance(before, transition, command),
    ensures concrete_holder_loss_transition(before, transition, command),
{
}

pub(crate) open spec fn exact_discontinuity_acceptance(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::FenceClockDiscontinuity,
) -> bool {
    matches!(before.state, crate::state::LeaseState::Active(_))
        && (command.observed_at.spec_epoch() != before.authority_time.spec_epoch()
            || command.observed_at.spec_tick_millis()
                < before.authority_time.spec_greatest_tick_millis())
        && transition.record.binding.matches_clock_discontinuity(command)
        && exact_fence_acceptance(
            before,
            transition,
            command.command_id,
            crate::LeaseTransitionKind::ClockDiscontinuity,
            Some(crate::FenceCause::ClockDiscontinuity),
            command.observed_at,
        )
}

pub closed spec fn concrete_discontinuity_transition(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::FenceClockDiscontinuity,
) -> bool {
    exact_discontinuity_acceptance(before, transition, command)
}

pub(crate) proof fn establish_discontinuity_transition(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::FenceClockDiscontinuity,
)
    requires exact_discontinuity_acceptance(before, transition, command),
    ensures concrete_discontinuity_transition(before, transition, command),
{
}

pub(crate) open spec fn exact_revoke_acceptance(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::RevokeLease,
) -> bool {
    super::concrete_claim_is_current(before, command.claim)
        && transition.record.binding.matches_revoke(command)
        && exact_fence_acceptance(
            before,
            transition,
            command.command_id,
            crate::LeaseTransitionKind::Revoked,
            Some(crate::FenceCause::Revoked),
            command.observed_at,
        )
}

pub closed spec fn concrete_revoke_transition(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::RevokeLease,
) -> bool {
    exact_revoke_acceptance(before, transition, command)
}

pub(crate) proof fn establish_revoke_transition(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::RevokeLease,
)
    requires exact_revoke_acceptance(before, transition, command),
    ensures concrete_revoke_transition(before, transition, command),
{
}

} // verus!
