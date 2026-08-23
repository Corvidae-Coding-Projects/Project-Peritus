//! Verified explicit duplication for unprivileged owned command evidence.

use super::{LeaseCommandBinding, LeasePermissionBinding, LeaseUseCommandBinding};
use super::command::LeaseCommandBindingData;
use vstd::prelude::*;

verus! {

impl LeasePermissionBinding {
    pub(crate) open spec fn exactly_duplicates(&self, source: &Self) -> bool {
        self.resource_id.spec_bytes() == source.resource_id.spec_bytes()
            && self.capability_name.spec_bytes() == source.capability_name.spec_bytes()
    }

    pub(crate) fn duplicate(&self) -> (duplicate: Self)
        ensures duplicate.exactly_duplicates(self),
    {
        Self {
            resource_id: self.resource_id,
            capability_name: self.capability_name.clone(),
        }
    }
}

impl LeaseUseCommandBinding {
    pub(crate) open spec fn exactly_duplicates(&self, source: &Self) -> bool {
        self.command_id == source.command_id
            && self.claim == source.claim
            && self.observed_at == source.observed_at
            && self.action_id == source.action_id
            && self.action_digest == source.action_digest
            && self.permission.exactly_duplicates(&source.permission)
            && self.actor_id == source.actor_id
            && self.role == source.role
            && self.environment_id == source.environment_id
            && self.scope_permissions@.len() == source.scope_permissions@.len()
            && forall |index: int| 0 <= index < source.scope_permissions@.len() ==>
                self.scope_permissions@[index]
                    .exactly_duplicates(&source.scope_permissions@[index])
            && self.revision == source.revision
            && self.validity == source.validity
            && self.scope_use_limit == source.scope_use_limit
            && self.used_at == source.used_at
            && self.transition_digest == source.transition_digest
            && self.previous_remaining == source.previous_remaining
            && self.successor_remaining == source.successor_remaining
            && self.successor_time_epoch == source.successor_time_epoch
            && self.successor_greatest_tick_millis
                == source.successor_greatest_tick_millis
            && self.successor_issued_at == source.successor_issued_at
            && self.successor_issuance_digest == source.successor_issuance_digest
            && self.successor_issuance_command_id == source.successor_issuance_command_id
    }

    pub(crate) fn duplicate(&self) -> (duplicate: Self)
        ensures duplicate.exactly_duplicates(self),
    {
        let mut scope_permissions: Vec<LeasePermissionBinding> = Vec::new();
        let mut index = 0_usize;
        while index < self.scope_permissions.len()
            invariant
                0 <= index <= self.scope_permissions.len(),
                scope_permissions@.len() == index,
                forall |prior: int| 0 <= prior < index ==>
                    scope_permissions@[prior]
                        .exactly_duplicates(&self.scope_permissions@[prior]),
            decreases self.scope_permissions.len() - index,
        {
            scope_permissions.push(self.scope_permissions[index].duplicate());
            index += 1;
        }
        let duplicate = Self {
            command_id: self.command_id,
            claim: self.claim,
            observed_at: self.observed_at,
            action_id: self.action_id,
            action_digest: self.action_digest,
            permission: self.permission.duplicate(),
            actor_id: self.actor_id,
            role: self.role,
            environment_id: self.environment_id,
            scope_permissions,
            revision: self.revision,
            validity: self.validity,
            scope_use_limit: self.scope_use_limit,
            used_at: self.used_at,
            transition_digest: self.transition_digest,
            previous_remaining: self.previous_remaining,
            successor_remaining: self.successor_remaining,
            successor_time_epoch: self.successor_time_epoch,
            successor_greatest_tick_millis: self.successor_greatest_tick_millis,
            successor_issued_at: self.successor_issued_at,
            successor_issuance_digest: self.successor_issuance_digest,
            successor_issuance_command_id: self.successor_issuance_command_id,
        };
        proof {
            assert(duplicate.scope_permissions@.len() == self.scope_permissions@.len());
            assert forall |permission_index: int|
                0 <= permission_index < self.scope_permissions@.len() implies
                    duplicate.scope_permissions@[permission_index]
                        .exactly_duplicates(&self.scope_permissions@[permission_index]) by {
            }
            assert(duplicate.exactly_duplicates(self));
        }
        duplicate
    }
}

impl LeaseCommandBinding {
    pub(crate) open spec fn exactly_duplicates(&self, source: &Self) -> bool {
        match (&self.data, &source.data) {
            (LeaseCommandBindingData::Mint(left), LeaseCommandBindingData::Mint(right)) => {
                *left == *right
            }
            (LeaseCommandBindingData::Acquire(left), LeaseCommandBindingData::Acquire(right)) => {
                *left == *right
            }
            (LeaseCommandBindingData::Renew(left), LeaseCommandBindingData::Renew(right)) => {
                **left == **right
            }
            (LeaseCommandBindingData::Use(left), LeaseCommandBindingData::Use(right)) => {
                left.exactly_duplicates(right)
            }
            (LeaseCommandBindingData::Release(left), LeaseCommandBindingData::Release(right)) => {
                **left == **right
            }
            (LeaseCommandBindingData::Expire(left), LeaseCommandBindingData::Expire(right)) => {
                *left == *right
            }
            (
                LeaseCommandBindingData::HolderLoss(left),
                LeaseCommandBindingData::HolderLoss(right),
            ) => **left == **right,
            (
                LeaseCommandBindingData::ClockDiscontinuity(left),
                LeaseCommandBindingData::ClockDiscontinuity(right),
            ) => *left == *right,
            (LeaseCommandBindingData::Revoke(left), LeaseCommandBindingData::Revoke(right)) => {
                **left == **right
            }
            (
                LeaseCommandBindingData::Reconcile(left),
                LeaseCommandBindingData::Reconcile(right),
            ) => **left == **right,
            _ => false,
        }
    }

    pub(crate) fn duplicate(&self) -> (duplicate: Self)
        ensures duplicate.exactly_duplicates(self),
    {
        let data = match &self.data {
            LeaseCommandBindingData::Mint(command) => LeaseCommandBindingData::Mint(*command),
            LeaseCommandBindingData::Acquire(command) => {
                LeaseCommandBindingData::Acquire(*command)
            }
            LeaseCommandBindingData::Renew(command) => {
                LeaseCommandBindingData::Renew(Box::new(**command))
            }
            LeaseCommandBindingData::Use(binding) => {
                LeaseCommandBindingData::Use(Box::new(binding.duplicate()))
            }
            LeaseCommandBindingData::Release(command) => {
                LeaseCommandBindingData::Release(Box::new(**command))
            }
            LeaseCommandBindingData::Expire(command) => LeaseCommandBindingData::Expire(*command),
            LeaseCommandBindingData::HolderLoss(command) => {
                LeaseCommandBindingData::HolderLoss(Box::new(**command))
            }
            LeaseCommandBindingData::ClockDiscontinuity(command) => {
                LeaseCommandBindingData::ClockDiscontinuity(*command)
            }
            LeaseCommandBindingData::Revoke(command) => {
                LeaseCommandBindingData::Revoke(Box::new(**command))
            }
            LeaseCommandBindingData::Reconcile(command) => {
                LeaseCommandBindingData::Reconcile(Box::new(**command))
            }
        };
        let duplicate = Self { data };
        proof {
            assert(duplicate.exactly_duplicates(self));
        }
        duplicate
    }
}

} // verus!
