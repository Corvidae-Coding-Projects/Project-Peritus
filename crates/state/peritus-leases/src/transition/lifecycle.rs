//! Mint, acquisition, and renewal reducers.

mod deadline;

use self::deadline::lease_deadline;

use super::{
    ensure_before_expiry, minted_transition, next_active_version, rejection,
    require_active_claim, require_phase, transition, validate_observation, AuthorityTimeAdvance,
    LeaseAggregate, LeaseError, LeaseState, LeaseTransition, LeaseTransitionKind, TransitionPlan,
};
use crate::state::ActiveLease;
use crate::{AcquireLease, LeasePhase, MintLease, RenewLease};
use crate::LeaseTransitionOutcome;
use peritus_policy::AuthorityTimeState;
use peritus_types::{Generation, RevisionNumber};
use vstd::prelude::*;

verus! {

impl LeaseAggregate {
    /// Mints a new available aggregate at generation and version one.
    ///
    /// # Errors
    ///
    /// This checked constructor is infallible for a validated [`MintLease`]; the result shape is
    /// uniform with the remaining reducers.
    pub fn mint(command: MintLease) -> (result: Result<LeaseTransition, LeaseError>)
        ensures
            match result {
                Ok(accepted) => crate::model::concrete_mint_transition(&accepted, command),
                Err(_) => false,
            },
    {
        let next = Self::from_parts(
            command.scope(),
            Generation::first(),
            RevisionNumber::first(),
            AuthorityTimeState::new(command.observed_at()),
            LeaseState::Available,
        );
        let accepted = minted_transition(command, next);
        proof {
            crate::model::concrete::establish_mint_transition(&accepted, command);
        }
        Ok(accepted)
    }

    /// Acquires an available generation for one exact actor/session holder.
    ///
    /// # Errors
    ///
    /// Returns a typed phase, authority-time, duration, version, or corrupt-state failure.
    pub fn acquire(
        self,
        command: AcquireLease,
    ) -> (result: LeaseTransitionOutcome)
        ensures crate::model::concrete::rejections::lifecycle::concrete_acquire_decision(
            &self,
            result,
            command,
        ),
    {
        let ghost before = self;
        if let Err(error) = require_phase(&self, LeasePhase::Available) {
            assert(crate::model::concrete::rejections::lifecycle::acquire_error(
                &self,
                command,
            ) == Some(error));
            return acquire_rejection(self, command, error);
        }
        if let Err(error) = validate_observation(&self.authority_time, command.observed_at()) {
            assert(crate::model::concrete::rejections::lifecycle::acquire_error(
                &self,
                command,
            ) == Some(error));
            return acquire_rejection(self, command, error);
        }
        let expires_at = match lease_deadline(command.observed_at(), command.duration()) {
            Ok(value) => value,
            Err(error) => {
                assert(crate::model::concrete::rejections::lifecycle::acquire_error(
                    &self,
                    command,
                ) == Some(error));
                return acquire_rejection(self, command, error);
            }
        };
        let _expires_tick = expires_at.tick_millis();
        assert(_expires_tick as int == expires_at.spec_tick_millis());
        assert(!crate::model::concrete::rejections::lifecycle::deadline_overflows(
            command.observed_at,
            command.duration,
        ));
        let version = match next_active_version(self.version) {
            Ok(value) => value,
            Err(error) => {
                assert(crate::model::concrete::rejections::lifecycle::acquire_error(
                    &self,
                    command,
                ) == Some(error));
                return acquire_rejection(self, command, error);
            }
        };
        let active = ActiveLease {
            holder: command.holder(),
            claim_version: RevisionNumber::first(),
            issued_at: command.observed_at(),
            expires_at,
        };
        let generation = self.generation;
        let command_id = command.command_id();
        let observed_at = command.observed_at();
        let binding = crate::LeaseCommandBinding::acquire(command);
        let accepted = match transition(self, TransitionPlan::new(
            command_id,
            version,
            generation,
            AuthorityTimeAdvance::Observe(observed_at),
            LeaseState::Active(active),
            LeaseTransitionKind::Acquired,
            binding,
        )) {
            LeaseTransitionOutcome::Accepted(accepted) => accepted,
            LeaseTransitionOutcome::Rejected(failure) => {
                proof { assert(false); }
                return LeaseTransitionOutcome::Rejected(failure);
            }
        };
        proof {
            crate::model::concrete::establish_acquire_transition(&before, &accepted, command);
            assert(crate::model::concrete::rejections::lifecycle::acquire_error(
                &before,
                command,
            ).is_none());
            crate::model::concrete::rejections::lifecycle::establish_acquire_acceptance(
                &before,
                &accepted,
                command,
            );
        }
        LeaseTransitionOutcome::Accepted(accepted)
    }

    /// Renews an exact unexpired claim and invalidates its prior claim version.
    ///
    /// # Errors
    ///
    /// Returns a typed claim, expiry, authority-time, duration, version, or corrupt-state failure.
    pub fn renew(
        self,
        command: RenewLease,
    ) -> (result: LeaseTransitionOutcome)
        ensures crate::model::concrete::rejections::lifecycle::concrete_renew_decision(
            &self,
            result,
            command,
        ),
    {
        let ghost before = self;
        let active = match require_active_claim(&self, command.claim()) {
            Ok(value) => value,
            Err(error) => {
                assert(crate::model::concrete::rejections::lifecycle::renew_error(
                    &self,
                    command,
                ) == Some(error));
                return renew_rejection(self, command, error);
            }
        };
        if let Err(error) = validate_observation(&self.authority_time, command.observed_at()) {
            assert(crate::model::concrete::rejections::lifecycle::renew_error(
                &self,
                command,
            ) == Some(error));
            return renew_rejection(self, command, error);
        }
        if let Err(error) = ensure_before_expiry(active.expires_at, command.observed_at()) {
            assert(crate::model::concrete::rejections::lifecycle::renew_error(
                &self,
                command,
            ) == Some(error));
            return renew_rejection(self, command, error);
        }
        let expires_at = match lease_deadline(command.observed_at(), command.duration()) {
            Ok(value) => value,
            Err(error) => {
                assert(crate::model::concrete::rejections::lifecycle::renew_error(
                    &self,
                    command,
                ) == Some(error));
                return renew_rejection(self, command, error);
            }
        };
        let _expires_tick = expires_at.tick_millis();
        assert(_expires_tick as int == expires_at.spec_tick_millis());
        assert(!crate::model::concrete::rejections::lifecycle::deadline_overflows(
            command.observed_at,
            command.duration,
        ));
        if expires_at.tick_millis() <= active.expires_at.tick_millis() {
            assert(crate::model::concrete::rejections::lifecycle::renew_error(
                &self,
                command,
            ) == Some(LeaseError::DeadlineNotExtended));
            return renew_rejection(self, command, LeaseError::DeadlineNotExtended);
        }
        let claim_version = match active.claim_version.checked_next() {
            Ok(value) => value,
            Err(_error) => {
                assert(crate::model::concrete::rejections::lifecycle::renew_error(
                    &self,
                    command,
                ) == Some(LeaseError::ClaimVersionExhausted));
                return renew_rejection(self, command, LeaseError::ClaimVersionExhausted);
            }
        };
        let version = match next_active_version(self.version) {
            Ok(value) => value,
            Err(error) => {
                assert(crate::model::concrete::rejections::lifecycle::renew_error(
                    &self,
                    command,
                ) == Some(error));
                return renew_rejection(self, command, error);
            }
        };
        let generation = self.generation;
        let command_id = command.command_id();
        let observed_at = command.observed_at();
        let binding = crate::LeaseCommandBinding::renew(command);
        let accepted = match transition(self, TransitionPlan::new(
            command_id,
            version,
            generation,
            AuthorityTimeAdvance::Observe(observed_at),
            LeaseState::Active(ActiveLease {
                holder: active.holder,
                claim_version,
                issued_at: command.observed_at(),
                expires_at,
            }),
            LeaseTransitionKind::Renewed,
            binding,
        )) {
            LeaseTransitionOutcome::Accepted(accepted) => accepted,
            LeaseTransitionOutcome::Rejected(failure) => {
                proof {
                    establish_late_renew_rejection(&before, &failure, command);
                }
                return LeaseTransitionOutcome::Rejected(failure);
            }
        };
        proof {
            establish_renew_decision(&before, &accepted, command);
        }
        LeaseTransitionOutcome::Accepted(accepted)
    }
}

proof fn establish_late_renew_rejection(
    before: &LeaseAggregate,
    failure: &crate::LeaseTransitionFailure,
    command: RenewLease,
)
    requires
        failure.spec_error() == LeaseError::CorruptState,
        crate::model::concrete::rejections::lifecycle::renew_error(before, command)
            == Some(failure.spec_error()),
        crate::model::concrete_rejection_preserves_input(before, failure),
    ensures crate::model::concrete::rejections::lifecycle::concrete_renew_decision(
        before,
        LeaseTransitionOutcome::Rejected(*failure),
        command,
    ),
{
    crate::model::concrete::rejections::lifecycle::establish_renew_rejection(
        before,
        failure,
        command,
        failure.spec_error(),
    );
}

proof fn establish_renew_decision(
    before: &LeaseAggregate,
    accepted: &LeaseTransition,
    command: RenewLease,
)
    requires
        crate::model::concrete::concrete_renew_edge(
            before,
            &accepted.next,
            accepted.record,
            command,
        ),
        crate::model::concrete::rejections::lifecycle::renew_error(before, command).is_none(),
    ensures crate::model::concrete::rejections::lifecycle::concrete_renew_decision(
        before,
        LeaseTransitionOutcome::Accepted(*accepted),
        command,
    ),
{
    crate::model::concrete::establish_renew_transition(before, accepted, command);
    crate::model::concrete::rejections::lifecycle::establish_renew_acceptance(
        before,
        accepted,
        command,
    );
}

const fn acquire_rejection(
    aggregate: LeaseAggregate,
    _command: AcquireLease,
    error: LeaseError,
) -> (result: LeaseTransitionOutcome)
    requires crate::model::concrete::rejections::lifecycle::acquire_error(
        &aggregate,
        _command,
    ) == Some(error),
    ensures crate::model::concrete::rejections::lifecycle::concrete_acquire_decision(
        &aggregate,
        result,
        _command,
    ),
{
    let ghost before = aggregate;
    let failure = rejection(aggregate, error);
    proof {
        crate::model::concrete::rejections::lifecycle::establish_acquire_rejection(
            &before,
            &failure,
            _command,
            error,
        );
    }
    LeaseTransitionOutcome::Rejected(failure)
}

const fn renew_rejection(
    aggregate: LeaseAggregate,
    _command: RenewLease,
    error: LeaseError,
) -> (result: LeaseTransitionOutcome)
    requires crate::model::concrete::rejections::lifecycle::renew_error(
        &aggregate,
        _command,
    ) == Some(error),
    ensures crate::model::concrete::rejections::lifecycle::concrete_renew_decision(
        &aggregate,
        result,
        _command,
    ),
{
    let ghost before = aggregate;
    let failure = rejection(aggregate, error);
    proof {
        crate::model::concrete::rejections::lifecycle::establish_renew_rejection(
            &before,
            &failure,
            _command,
            error,
        );
    }
    LeaseTransitionOutcome::Rejected(failure)
}

} // verus!
