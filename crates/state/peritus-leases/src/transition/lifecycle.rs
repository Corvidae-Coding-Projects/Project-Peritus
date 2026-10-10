//! Mint, acquisition, and renewal reducers.

mod deadline;

use self::deadline::lease_deadline;

use super::{
    AuthorityTimeAdvance, LeaseAggregate, LeaseError, LeaseState, LeaseTransition,
    LeaseTransitionKind, TransitionPlan, ensure_before_expiry, minted_transition,
    next_active_version, rejection, require_active_claim, require_phase, transition,
    validate_observation,
};
use crate::LeaseTransitionOutcome;
use crate::state::ActiveLease;
use crate::{AcquireLease, LeasePhase, MintLease, RenewLease};
use peritus_policy::AuthorityTimeState;
use peritus_types::{Generation, RevisionNumber};

impl LeaseAggregate {
    #[doc = " Mints a new available aggregate at generation and version one."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " This checked constructor is infallible for a validated [`MintLease`]; the result shape is"]
    #[doc = " uniform with the remaining reducers."]
    pub fn mint(command: MintLease) -> Result<LeaseTransition, LeaseError> {
        let next = Self::from_parts(
            command.scope(),
            Generation::first(),
            RevisionNumber::first(),
            AuthorityTimeState::new(command.observed_at()),
            LeaseState::Available,
        );
        let accepted = minted_transition(command, next);
        Ok(accepted)
    }
    #[doc = " Acquires an available generation for one exact actor/session holder."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed phase, authority-time, duration, version, or corrupt-state failure."]
    pub fn acquire(self, command: AcquireLease) -> LeaseTransitionOutcome {
        if let Err(error) = require_phase(&self, LeasePhase::Available) {
            return acquire_rejection(self, command, error);
        }
        if let Err(error) = validate_observation(&self.authority_time, command.observed_at()) {
            return acquire_rejection(self, command, error);
        }
        let expires_at = match lease_deadline(command.observed_at(), command.duration()) {
            Ok(value) => value,
            Err(error) => {
                return acquire_rejection(self, command, error);
            }
        };
        let _expires_tick = expires_at.tick_millis();
        let version = match next_active_version(self.version) {
            Ok(value) => value,
            Err(error) => {
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
        let accepted = match transition(
            self,
            TransitionPlan::new(
                command_id,
                version,
                generation,
                AuthorityTimeAdvance::Observe(observed_at),
                LeaseState::Active(active),
                LeaseTransitionKind::Acquired,
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
    #[doc = " Renews an exact unexpired claim and invalidates its prior claim version."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed claim, expiry, authority-time, duration, version, or corrupt-state failure."]
    pub fn renew(self, command: RenewLease) -> LeaseTransitionOutcome {
        let active = match require_active_claim(&self, command.claim()) {
            Ok(value) => value,
            Err(error) => {
                return renew_rejection(self, command, error);
            }
        };
        if let Err(error) = validate_observation(&self.authority_time, command.observed_at()) {
            return renew_rejection(self, command, error);
        }
        if let Err(error) = ensure_before_expiry(active.expires_at, command.observed_at()) {
            return renew_rejection(self, command, error);
        }
        let expires_at = match lease_deadline(command.observed_at(), command.duration()) {
            Ok(value) => value,
            Err(error) => {
                return renew_rejection(self, command, error);
            }
        };
        let _expires_tick = expires_at.tick_millis();
        if expires_at.tick_millis() <= active.expires_at.tick_millis() {
            return renew_rejection(self, command, LeaseError::DeadlineNotExtended);
        }
        let claim_version = match active.claim_version.checked_next() {
            Ok(value) => value,
            Err(_error) => {
                return renew_rejection(self, command, LeaseError::ClaimVersionExhausted);
            }
        };
        let version = match next_active_version(self.version) {
            Ok(value) => value,
            Err(error) => {
                return renew_rejection(self, command, error);
            }
        };
        let generation = self.generation;
        let command_id = command.command_id();
        let observed_at = command.observed_at();
        let binding = crate::LeaseCommandBinding::renew(command);
        let accepted = match transition(
            self,
            TransitionPlan::new(
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
            ),
        ) {
            LeaseTransitionOutcome::Accepted(accepted) => accepted,
            LeaseTransitionOutcome::Rejected(failure) => {
                return LeaseTransitionOutcome::Rejected(failure);
            }
        };
        LeaseTransitionOutcome::Accepted(accepted)
    }
}
const fn acquire_rejection(
    aggregate: LeaseAggregate,
    _command: AcquireLease,
    error: LeaseError,
) -> LeaseTransitionOutcome {
    let failure = rejection(aggregate, error);
    LeaseTransitionOutcome::Rejected(failure)
}
const fn renew_rejection(
    aggregate: LeaseAggregate,
    _command: RenewLease,
    error: LeaseError,
) -> LeaseTransitionOutcome {
    let failure = rejection(aggregate, error);
    LeaseTransitionOutcome::Rejected(failure)
}
