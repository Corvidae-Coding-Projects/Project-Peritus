//! Voluntary release reducer and exact command decision proof.

use super::super::fencing_apply::fence;
use super::super::fencing_validation::validate_release_quiescence;
use super::super::{
    LeaseAggregate, LeaseTransitionKind, require_active_claim, validate_observation,
};
use crate::{FenceCause, LeaseTransitionOutcome, ReleaseLease};

impl LeaseAggregate {
    #[doc = " Releases and fences one exact claim, reconciling unless exact quiescence is supplied."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Returns a typed claim, quiescence, authority-time, generation, version, or state failure."]
    pub fn release(self, command: ReleaseLease) -> LeaseTransitionOutcome {
        let _active = match require_active_claim(&self, command.claim()) {
            Ok(active) => active,
            Err(error) => {
                return super::super::fencing_rejections::release(self, &command, error);
            }
        };
        if let Err(error) = validate_observation(&self.authority_time, command.observed_at()) {
            return super::super::fencing_rejections::release(self, &command, error);
        }
        let direct_available = match validate_release_quiescence(&command) {
            Ok(value) => value,
            Err(error) => {
                return super::super::fencing_rejections::release(self, &command, error);
            }
        };
        let binding = crate::LeaseCommandBinding::release(&command);
        let result = fence(
            self,
            command.command_id(),
            command.observed_at(),
            if direct_available { None } else { Some(FenceCause::ReleasedWithoutQuiescence) },
            if direct_available {
                LeaseTransitionKind::ReleasedAvailable
            } else {
                LeaseTransitionKind::ReleasedReconciling
            },
            binding,
        );
        result
    }
}
