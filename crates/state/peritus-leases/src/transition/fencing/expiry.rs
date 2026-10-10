//! Deadline-expiry reducer and exact command decision proof.

use super::super::fencing_apply::fence;
use super::super::{
    LeaseAggregate, LeaseError, LeaseTransitionKind, require_active, validate_observation,
};
use crate::{ExpireLease, FenceCause, LeaseTransitionOutcome};

impl LeaseAggregate {
    #[doc = " Fences at the exact expiry boundary or later and enters reconciliation."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Returns a typed phase, expiry, authority-time, generation, version, or state failure."]
    pub fn expire(self, command: ExpireLease) -> LeaseTransitionOutcome {
        let active = match require_active(&self) {
            Ok(value) => value,
            Err(error) => {
                return super::super::fencing_rejections::expiry(self, command, error);
            }
        };
        if let Err(error) = validate_observation(&self.authority_time, command.observed_at()) {
            return super::super::fencing_rejections::expiry(self, command, error);
        }
        if command.observed_at().epoch().get() != active.expires_at.epoch().get() {
            return super::super::fencing_rejections::expiry(
                self,
                command,
                LeaseError::CorruptState,
            );
        }
        if command.observed_at().tick_millis() < active.expires_at.tick_millis() {
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
        result
    }
}
