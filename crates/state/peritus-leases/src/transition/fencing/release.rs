//! Voluntary release reducer and exact command decision proof.

use super::super::{require_active_claim, validate_observation, LeaseAggregate,
    LeaseTransitionKind};
use super::super::fencing_apply::fence;
use super::super::fencing_validation::validate_release_quiescence;
use crate::{FenceCause, LeaseTransitionOutcome, ReleaseLease};
use vstd::prelude::*;

verus! {

impl LeaseAggregate {
    /// Releases and fences one exact claim, reconciling unless exact quiescence is supplied.
    ///
    /// # Errors
    /// Returns a typed claim, quiescence, authority-time, generation, version, or state failure.
    pub fn release(
        self,
        command: ReleaseLease,
    ) -> (result: LeaseTransitionOutcome)
        ensures crate::model::concrete::rejections::fencing::release::concrete_release_decision(
            &self,
            result,
            command,
        ),
    {
        let ghost before = self;
        let _active = match require_active_claim(&self, command.claim()) {
            Ok(active) => active,
            Err(error) => {
                assert(crate::model::concrete::rejections::fencing::release::release_error(
                    &self,
                    command,
                ) == Some(error));
                return super::super::fencing_rejections::release(self, &command, error);
            }
        };
        if let Err(error) = validate_observation(&self.authority_time, command.observed_at()) {
            assert(crate::model::concrete::rejections::fencing::release::release_error(
                &self,
                command,
            ) == Some(error));
            return super::super::fencing_rejections::release(self, &command, error);
        }
        let direct_available = match validate_release_quiescence(&command) {
            Ok(value) => value,
            Err(error) => {
                assert(crate::model::concrete::rejections::fencing::release::release_error(
                    &self,
                    command,
                ) == Some(error));
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
        proof {
            establish_release_result(&before, &result, command, direct_available);
        }
        result
    }
}

proof fn establish_release_result(
    before: &LeaseAggregate,
    result: &LeaseTransitionOutcome,
    command: ReleaseLease,
    direct_available: bool,
)
    requires
        crate::transition::active_claim_error(before, command.claim).is_none(),
        crate::transition::observation_error(
            &before.authority_time,
            command.observed_at,
        ).is_none(),
        match command.spec_quiescence() {
            Some(evidence) => {
                direct_available
                    && crate::model::concrete_claim_matches(
                        evidence.spec_claim(),
                        command.claim,
                    )
            }
            None => !direct_available,
        },
        match *result {
            LeaseTransitionOutcome::Accepted(accepted) => {
                crate::transition::fencing_model::fence_error(
                    before,
                    command.observed_at,
                    if direct_available {
                        None
                    } else {
                        Some(FenceCause::ReleasedWithoutQuiescence)
                    },
                    if direct_available {
                        LeaseTransitionKind::ReleasedAvailable
                    } else {
                        LeaseTransitionKind::ReleasedReconciling
                    },
                ).is_none()
                    && crate::model::concrete_fence_decision(
                    before,
                    &accepted.next,
                    accepted.record,
                    command.command_id,
                    if direct_available {
                        LeaseTransitionKind::ReleasedAvailable
                    } else {
                        LeaseTransitionKind::ReleasedReconciling
                    },
                    if direct_available {
                        None
                    } else {
                        Some(FenceCause::ReleasedWithoutQuiescence)
                    },
                ) && crate::model::concrete_fence_time_observed(
                    before,
                    &accepted.next,
                    command.observed_at,
                    if direct_available {
                        None
                    } else {
                        Some(FenceCause::ReleasedWithoutQuiescence)
                    },
                ) && accepted.record.binding.matches_release(command)
            }
            LeaseTransitionOutcome::Rejected(failure) => {
                crate::transition::fencing_model::fence_error(
                    before,
                    command.observed_at,
                    if direct_available {
                        None
                    } else {
                        Some(FenceCause::ReleasedWithoutQuiescence)
                    },
                    if direct_available {
                        LeaseTransitionKind::ReleasedAvailable
                    } else {
                        LeaseTransitionKind::ReleasedReconciling
                    },
                )
                    == Some(failure.spec_error())
                    && crate::model::concrete_rejection_preserves_input(before, &failure)
            }
        },
    ensures crate::model::concrete::rejections::fencing::release::concrete_release_decision(
        before,
        *result,
        command,
    ),
{
    if let LeaseTransitionOutcome::Accepted(ref accepted) = *result {
        match command.spec_quiescence() {
            Some(_) => {
                assert(crate::model::concrete::fence_commands::exact_fence_acceptance(
                    before,
                    accepted,
                    command.command_id,
                    LeaseTransitionKind::ReleasedAvailable,
                    None,
                    command.observed_at,
                ));
            }
            None => {
                assert(crate::model::concrete::fence_commands::exact_fence_acceptance(
                    before,
                    accepted,
                    command.command_id,
                    LeaseTransitionKind::ReleasedReconciling,
                    Some(FenceCause::ReleasedWithoutQuiescence),
                    command.observed_at,
                ));
            }
        }
        assert(crate::model::concrete::fence_commands::exact_release_acceptance(
            before,
            accepted,
            command,
        ));
        crate::model::concrete::fence_commands::establish_release_transition(
            before,
            accepted,
            command,
        );
    }
    match *result {
        LeaseTransitionOutcome::Accepted(ref accepted) => {
            assert(crate::model::concrete::rejections::fencing::release::release_error(
                before,
                command,
            ).is_none());
            crate::model::concrete::rejections::fencing::release::establish_release_acceptance(
                before,
                accepted,
                command,
            );
        }
        LeaseTransitionOutcome::Rejected(ref failure) => {
            assert(crate::model::concrete::rejections::fencing::release::release_error(
                before,
                command,
            ) == Some(failure.spec_error()));
            crate::model::concrete::rejections::fencing::release::establish_release_rejection(
                before,
                failure,
                command,
                failure.spec_error(),
            );
        }
    }
}

} // verus!
