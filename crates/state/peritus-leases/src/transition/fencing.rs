//! Holder-loss, clock-discontinuity, and revocation fencing reducers.

mod expiry;
mod release;

use super::{require_active, require_active_claim, validate_observation, LeaseAggregate,
    LeaseError, LeaseTransitionKind};
use super::fencing_apply::fence;
use crate::{FenceCause, FenceClockDiscontinuity, FenceHolderLoss, LeaseTransitionOutcome,
    RevokeLease};
use vstd::prelude::*;

verus! {

impl LeaseAggregate {
    /// Fences after exact matching holder-loss evidence and enters reconciliation.
    ///
    /// # Errors
    /// Returns a typed evidence, claim, authority-time, generation, version, or state failure.
    pub fn fence_holder_loss(
        self,
        command: FenceHolderLoss,
    ) -> (result: LeaseTransitionOutcome)
        ensures crate::model::concrete::rejections::fencing::holder_loss::concrete_holder_loss_decision(
            &self,
            result,
            command,
        ),
    {
        let ghost before = self;
        if let Err(error) = require_active(&self) {
            assert(crate::model::concrete::rejections::fencing::holder_loss::holder_loss_error(
                &self,
                command,
            ) == Some(error));
            return super::fencing_rejections::holder_loss(self, command, error);
        }
        if let Err(_error) = require_active_claim(&self, command.evidence().claim()) {
            assert(crate::model::concrete::rejections::fencing::holder_loss::holder_loss_error(
                &self,
                command,
            ) == Some(LeaseError::HolderLossMismatch));
            return super::fencing_rejections::holder_loss(
                self,
                command,
                LeaseError::HolderLossMismatch,
            );
        }
        if let Err(error) = validate_observation(&self.authority_time, command.observed_at()) {
            assert(crate::model::concrete::rejections::fencing::holder_loss::holder_loss_error(
                &self,
                command,
            ) == Some(error));
            return super::fencing_rejections::holder_loss(self, command, error);
        }
        let binding = crate::LeaseCommandBinding::holder_loss(command);
        let result = fence(
            self,
            command.command_id(),
            command.observed_at(),
            Some(FenceCause::HolderLost),
            LeaseTransitionKind::HolderLost,
            binding,
        );
        proof {
            if let LeaseTransitionOutcome::Accepted(ref accepted) = result {
                assert(accepted.record.binding.matches_holder_loss(command));
                assert(crate::model::concrete::fence_commands::exact_holder_loss_acceptance(
                    &before,
                    accepted,
                    command,
                ));
                crate::model::concrete::fence_commands::establish_holder_loss_transition(
                    &before,
                    accepted,
                    command,
                );
            }
            match result {
                LeaseTransitionOutcome::Accepted(ref accepted) => {
                    assert(crate::model::concrete::rejections::fencing::holder_loss::holder_loss_error(
                        &before,
                        command,
                    ).is_none());
                    crate::model::concrete::rejections::fencing::holder_loss::establish_holder_loss_acceptance(
                        &before,
                        accepted,
                        command,
                    );
                }
                LeaseTransitionOutcome::Rejected(ref failure) => {
                    assert(crate::model::concrete::rejections::fencing::holder_loss::holder_loss_error(
                        &before,
                        command,
                    ) == Some(failure.spec_error()));
                    crate::model::concrete::rejections::fencing::holder_loss::establish_holder_loss_rejection(
                        &before,
                        failure,
                        command,
                        failure.spec_error(),
                    );
                }
            }
        }
        result
    }

    /// Explicitly fences after an epoch change or same-epoch regression.
    ///
    /// # Errors
    /// Returns a typed phase, discontinuity, generation, version, or state failure.
    pub fn fence_clock_discontinuity(
        self,
        command: FenceClockDiscontinuity,
    ) -> (result: LeaseTransitionOutcome)
        ensures crate::model::concrete::rejections::fencing::discontinuity::concrete_discontinuity_decision(
            &self,
            result,
            command,
        ),
    {
        let ghost before = self;
        if let Err(error) = require_active(&self) {
            assert(crate::model::concrete::rejections::fencing::discontinuity::discontinuity_error(
                &self,
                command,
            ) == Some(error));
            return super::fencing_rejections::discontinuity(self, command, error);
        }
        let observed = command.observed_at();
        let same_epoch = observed.epoch().get() == self.authority_time.epoch().get();
        let discontinuous = !same_epoch
            || observed.tick_millis() < self.authority_time.greatest_tick_millis();
        if !discontinuous {
            assert(crate::model::concrete::rejections::fencing::discontinuity::discontinuity_error(
                &self,
                command,
            ) == Some(LeaseError::NoClockDiscontinuity));
            return super::fencing_rejections::discontinuity(
                self,
                command,
                LeaseError::NoClockDiscontinuity,
            );
        }
        let binding = crate::LeaseCommandBinding::clock_discontinuity(command);
        let result = fence(
            self,
            command.command_id(),
            observed,
            Some(FenceCause::ClockDiscontinuity),
            LeaseTransitionKind::ClockDiscontinuity,
            binding,
        );
        proof {
            if let LeaseTransitionOutcome::Accepted(ref accepted) = result {
                assert(accepted.record.binding.matches_clock_discontinuity(command));
                assert(crate::model::concrete::fence_commands::exact_discontinuity_acceptance(
                    &before,
                    accepted,
                    command,
                ));
                crate::model::concrete::fence_commands::establish_discontinuity_transition(
                    &before,
                    accepted,
                    command,
                );
            }
            match result {
                LeaseTransitionOutcome::Accepted(ref accepted) => {
                    assert(crate::model::concrete::rejections::fencing::discontinuity::discontinuity_error(
                        &before,
                        command,
                    ).is_none());
                    crate::model::concrete::rejections::fencing::discontinuity::establish_discontinuity_acceptance(
                        &before,
                        accepted,
                        command,
                    );
                }
                LeaseTransitionOutcome::Rejected(ref failure) => {
                    assert(crate::model::concrete::rejections::fencing::discontinuity::discontinuity_error(
                        &before,
                        command,
                    ) == Some(failure.spec_error()));
                    crate::model::concrete::rejections::fencing::discontinuity::establish_discontinuity_rejection(
                        &before,
                        failure,
                        command,
                        failure.spec_error(),
                    );
                }
            }
        }
        result
    }

    /// Fences one exact active claim after a separately authorized revocation.
    ///
    /// # Errors
    /// Returns a typed claim, authority-time, generation, version, or state failure.
    pub fn revoke(
        self,
        command: RevokeLease,
    ) -> (result: LeaseTransitionOutcome)
        ensures crate::model::concrete::rejections::fencing::revoke::concrete_revoke_decision(
            &self,
            result,
            command,
        ),
    {
        let ghost before = self;
        if let Err(error) = require_active_claim(&self, command.claim()) {
            assert(crate::model::concrete::rejections::fencing::revoke::revoke_error(
                &self,
                command,
            ) == Some(error));
            return super::fencing_rejections::revoke(self, command, error);
        }
        if let Err(error) = validate_observation(&self.authority_time, command.observed_at()) {
            assert(crate::model::concrete::rejections::fencing::revoke::revoke_error(
                &self,
                command,
            ) == Some(error));
            return super::fencing_rejections::revoke(self, command, error);
        }
        let _evidence_id = command.evidence_id();
        let binding = crate::LeaseCommandBinding::revoke(command);
        let result = fence(
            self,
            command.command_id(),
            command.observed_at(),
            Some(FenceCause::Revoked),
            LeaseTransitionKind::Revoked,
            binding,
        );
        proof {
            if let LeaseTransitionOutcome::Accepted(ref accepted) = result {
                assert(accepted.record.binding.matches_revoke(command));
                assert(crate::model::concrete::fence_commands::exact_revoke_acceptance(
                    &before,
                    accepted,
                    command,
                ));
                crate::model::concrete::fence_commands::establish_revoke_transition(
                    &before,
                    accepted,
                    command,
                );
            }
            match result {
                LeaseTransitionOutcome::Accepted(ref accepted) => {
                    assert(crate::model::concrete::rejections::fencing::revoke::revoke_error(
                        &before,
                        command,
                    ).is_none());
                    crate::model::concrete::rejections::fencing::revoke::establish_revoke_acceptance(
                        &before,
                        accepted,
                        command,
                    );
                }
                LeaseTransitionOutcome::Rejected(ref failure) => {
                    assert(crate::model::concrete::rejections::fencing::revoke::revoke_error(
                        &before,
                        command,
                    ) == Some(failure.spec_error()));
                    crate::model::concrete::rejections::fencing::revoke::establish_revoke_rejection(
                        &before,
                        failure,
                        command,
                        failure.spec_error(),
                    );
                }
            }
        }
        result
    }
}

pub closed spec fn fence_result_refines(
    before: &LeaseAggregate,
    result: LeaseTransitionOutcome,
    command_id: peritus_types::CommandId,
    normal_kind: LeaseTransitionKind,
    cause: Option<FenceCause>,
    observed_at: peritus_policy::AuthorityInstant,
) -> bool {
    match result {
        LeaseTransitionOutcome::Accepted(accepted) => {
            crate::model::concrete_fence_decision(
                before,
                &accepted.next,
                accepted.record,
                command_id,
                normal_kind,
                cause,
            ) && crate::model::concrete_fence_time_observed(
                before,
                &accepted.next,
                observed_at,
                cause,
            )
        }
        LeaseTransitionOutcome::Rejected(failure) => {
            crate::model::concrete_rejection_preserves_input(before, &failure)
        }
    }
}

} // verus!
