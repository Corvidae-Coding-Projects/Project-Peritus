//! Verified explicit duplication for unprivileged owned command evidence.

use super::command::LeaseCommandBindingData;
use super::{LeaseCommandBinding, LeasePermissionBinding, LeaseUseCommandBinding};

impl LeasePermissionBinding {
    pub(crate) fn duplicate(&self) -> Self {
        Self { resource_id: self.resource_id, capability_name: self.capability_name.clone() }
    }
}
impl LeaseUseCommandBinding {
    pub(crate) fn duplicate(&self) -> Self {
        let mut scope_permissions: Vec<LeasePermissionBinding> = Vec::new();
        let mut index = 0_usize;
        while index < self.scope_permissions.len() {
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
        duplicate
    }
}
impl LeaseCommandBinding {
    pub(crate) fn duplicate(&self) -> Self {
        let data = match &self.data {
            LeaseCommandBindingData::Mint(command) => LeaseCommandBindingData::Mint(*command),
            LeaseCommandBindingData::Acquire(command) => LeaseCommandBindingData::Acquire(*command),
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
        duplicate
    }
}
