//! Deadline-expiry reducer and exact command decision proof.

use super::super::{require_active, validate_observation, LeaseAggregate, LeaseError,
    LeaseTransitionKind};
use super::super::fencing_apply::fence;
use crate::{ExpireLease, FenceCause, LeaseTransitionOutcome};
use vstd::prelude::*;

verus! {

impl LeaseAggregate {
    /// Fences at the exact expiry boundary or later and enters reconciliation.
    ///
    /// # Errors
    /// Returns a typed phase, expiry, authority-time, generation, version, or state failure.
    pub fn expire(
        self,
        command: ExpireLease,
    ) -> (result: LeaseTransitionOutcome)
        ensures crate::model::concrete::rejections::fencing::expiry::concrete_expiry_decision(
            &self,
            result,
            command,
        ),
    {
        let ghost before = self;
        let active = match require_active(&self) {
            Ok(value) => value,
            Err(error) => {
                assert(crate::model::concrete::rejections::fencing::expiry::expiry_error(
                    &self,
                    command,
                ) == Some(error));
                return super::super::fencing_rejections::expiry(self, command, error);
            }
        };
        if let Err(error) = validate_observation(&self.authority_time, command.observed_at()) {
            assert(crate::model::concrete::rejections::fencing::expiry::expiry_error(
                &self,
                command,
            ) == Some(error));
            return super::super::fencing_rejections::expiry(self, command, error);
        }
        if command.observed_at().epoch().get() != active.expires_at.epoch().get() {
            assert(crate::model::concrete::rejections::fencing::expiry::expiry_error(
                &self,
                command,
            ) == Some(LeaseError::CorruptState));
            return super::super::fencing_rejections::expiry(
                self,
                command,
                LeaseError::CorruptState,
            );
        }
        if command.observed_at().tick_millis() < active.expires_at.tick_millis() {
            assert(crate::model::concrete::rejections::fencing::expiry::expiry_error(
                &self,
                command,
            ) == Some(LeaseError::LeaseNotExpired));
            return super::super::fencing_rejections::expiry(
                self,
                command,
                LeaseError::LeaseNotExpired,
            );
        }
        let binding = crate::LeaseCommandBinding::expire(command);
        let result = fence(
            self,
            command.command_id(),
            command.observed_at(),
            Some(FenceCause::Expired),
            LeaseTransitionKind::Expired,
            binding,
        );
        proof {
            if let LeaseTransitionOutcome::Accepted(ref accepted) = result {
                assert(accepted.record.binding.matches_expire(command));
                assert(crate::model::concrete::fence_commands::exact_expire_acceptance(
                    &before,
                    accepted,
                    command,
                ));
                crate::model::concrete::fence_commands::establish_expire_transition(
                    &before,
                    accepted,
                    command,
                );
            }
            match result {
                LeaseTransitionOutcome::Accepted(ref accepted) => {
                    assert(crate::model::concrete::rejections::fencing::expiry::expiry_error(
                        &before,
                        command,
                    ).is_none());
                    crate::model::concrete::rejections::fencing::expiry::establish_expiry_acceptance(
                        &before,
                        accepted,
                        command,
                    );
                }
                LeaseTransitionOutcome::Rejected(ref failure) => {
                    assert(crate::model::concrete::rejections::fencing::expiry::expiry_error(
                        &before,
                        command,
                    ) == Some(failure.spec_error()));
                    crate::model::concrete::rejections::fencing::expiry::establish_expiry_rejection(
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

} // verus!
