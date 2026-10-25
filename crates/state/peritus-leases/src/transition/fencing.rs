//! Holder-loss, clock-discontinuity, and revocation fencing reducers.

mod expiry;
mod release;

use super::fencing_apply::fence;
use super::{
    LeaseAggregate, LeaseError, LeaseTransitionKind, require_active, require_active_claim,
    validate_observation,
};
use crate::{
    FenceCause, FenceClockDiscontinuity, FenceHolderLoss, LeaseTransitionOutcome, RevokeLease,
};

impl LeaseAggregate {
    #[doc = " Fences after exact matching holder-loss evidence and enters reconciliation."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Returns a typed evidence, claim, authority-time, generation, version, or state failure."]
    pub fn fence_holder_loss(self, command: FenceHolderLoss) -> LeaseTransitionOutcome {
        if let Err(error) = require_active(&self) {
            return super::fencing_rejections::holder_loss(self, command, error);
        }
        if let Err(_error) = require_active_claim(&self, command.evidence().claim()) {
            return super::fencing_rejections::holder_loss(
                self,
                command,
                LeaseError::HolderLossMismatch,
            );
        }
        if let Err(error) = validate_observation(&self.authority_time, command.observed_at()) {
            return super::fencing_rejections::holder_loss(self, command, error);
        }
        let binding = crate::LeaseCommandBinding::holder_loss(command);

        fence(
            self,
            command.command_id(),
            command.observed_at(),
            Some(FenceCause::HolderLost),
            LeaseTransitionKind::HolderLost,
            binding,
        )
    }
    #[doc = " Explicitly fences after an epoch change or same-epoch regression."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Returns a typed phase, discontinuity, generation, version, or state failure."]
    pub fn fence_clock_discontinuity(
        self,
        command: FenceClockDiscontinuity,
    ) -> LeaseTransitionOutcome {
        if let Err(error) = require_active(&self) {
            return super::fencing_rejections::discontinuity(self, command, error);
        }
        let observed = command.observed_at();
        let same_epoch = observed.epoch().get() == self.authority_time.epoch().get();
        let discontinuous =
            !same_epoch || observed.tick_millis() < self.authority_time.greatest_tick_millis();
        if !discontinuous {
            return super::fencing_rejections::discontinuity(
                self,
                command,
                LeaseError::NoClockDiscontinuity,
            );
        }
        let binding = crate::LeaseCommandBinding::clock_discontinuity(command);

        fence(
            self,
            command.command_id(),
            observed,
            Some(FenceCause::ClockDiscontinuity),
            LeaseTransitionKind::ClockDiscontinuity,
            binding,
        )
    }
    #[doc = " Fences one exact active claim after a separately authorized revocation."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Returns a typed claim, authority-time, generation, version, or state failure."]
    pub fn revoke(self, command: RevokeLease) -> LeaseTransitionOutcome {
        if let Err(error) = require_active_claim(&self, command.claim()) {
            return super::fencing_rejections::revoke(self, command, error);
        }
        if let Err(error) = validate_observation(&self.authority_time, command.observed_at()) {
            return super::fencing_rejections::revoke(self, command, error);
        }
        let _evidence_id = command.evidence_id();
        let binding = crate::LeaseCommandBinding::revoke(command);

        fence(
            self,
            command.command_id(),
            command.observed_at(),
            Some(FenceCause::Revoked),
            LeaseTransitionKind::Revoked,
            binding,
        )
    }
}
