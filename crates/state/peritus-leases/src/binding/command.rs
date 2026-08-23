//! Closed command-family binding carried by transition records.
#![allow(
    missing_docs,
    reason = "pinned Cargo-Verus synthesizes undocumented accessors for documented payload variants"
)]

use super::LeaseUseCommandBinding;
use crate::{
    AcquireLease, ExpireLease, FenceClockDiscontinuity, FenceHolderLoss, MintLease,
    ReconcileLease, ReleaseLease, RenewLease, RevokeLease, UseLease,
};
use vstd::prelude::*;

verus! {

/// Stable tag for the exact source command projected into an accepted lease plan.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LeaseCommandBindingKind {
    Mint,
    Acquire,
    Renew,
    Use,
    Release,
    Expire,
    HolderLoss,
    ClockDiscontinuity,
    Revoke,
    Reconcile,
}

#[derive(Debug, Eq, Hash, PartialEq)]
pub enum LeaseCommandBindingData {
    Mint(MintLease),
    Acquire(AcquireLease),
    Renew(Box<RenewLease>),
    Use(Box<LeaseUseCommandBinding>),
    Release(Box<ReleaseLease>),
    Expire(ExpireLease),
    HolderLoss(Box<FenceHolderLoss>),
    ClockDiscontinuity(FenceClockDiscontinuity),
    Revoke(Box<RevokeLease>),
    Reconcile(Box<ReconcileLease>),
}

/// Exact, unprivileged source-command binding carried by every accepted transition and CAS echo.
///
/// Construction is private to accepted reducers. Public projections are immutable evidence only;
/// they cannot create a lease, durable receipt, holder handle, or effect permit.
#[derive(Debug, Eq, Hash, PartialEq)]
pub struct LeaseCommandBinding {
    pub(crate) data: LeaseCommandBindingData,
}

impl LeaseCommandBinding {
    pub(crate) const fn mint(command: MintLease) -> (binding: Self)
        ensures binding.matches_mint(command),
    {
        Self { data: LeaseCommandBindingData::Mint(command) }
    }
    pub(crate) const fn acquire(command: AcquireLease) -> (binding: Self)
        ensures binding.matches_acquire(command),
    {
        Self { data: LeaseCommandBindingData::Acquire(command) }
    }
    pub(crate) fn renew(command: RenewLease) -> (binding: Self)
        ensures binding.matches_renew(command),
    {
        Self { data: LeaseCommandBindingData::Renew(Box::new(command)) }
    }
    pub(crate) fn use_command(command: &UseLease) -> (binding: Self)
        ensures binding.matches_use(command),
    {
        Self {
            data: LeaseCommandBindingData::Use(Box::new(
                LeaseUseCommandBinding::from_command(command),
            )),
        }
    }
    pub(crate) fn release(command: &ReleaseLease) -> (binding: Self)
        ensures binding.matches_release(*command),
    {
        Self { data: LeaseCommandBindingData::Release(Box::new(*command)) }
    }
    pub(crate) const fn expire(command: ExpireLease) -> (binding: Self)
        ensures binding.matches_expire(command),
    {
        Self { data: LeaseCommandBindingData::Expire(command) }
    }
    pub(crate) fn holder_loss(command: FenceHolderLoss) -> (binding: Self)
        ensures binding.matches_holder_loss(command),
    {
        Self { data: LeaseCommandBindingData::HolderLoss(Box::new(command)) }
    }
    pub(crate) const fn clock_discontinuity(
        command: FenceClockDiscontinuity,
    ) -> (binding: Self)
        ensures binding.matches_clock_discontinuity(command),
    {
        Self { data: LeaseCommandBindingData::ClockDiscontinuity(command) }
    }
    pub(crate) fn revoke(command: RevokeLease) -> (binding: Self)
        ensures binding.matches_revoke(command),
    {
        Self { data: LeaseCommandBindingData::Revoke(Box::new(command)) }
    }
    pub(crate) fn reconcile(command: &ReconcileLease) -> (binding: Self)
        ensures binding.matches_reconcile(*command),
    {
        Self { data: LeaseCommandBindingData::Reconcile(Box::new(*command)) }
    }

    pub(crate) open spec fn matches_mint(&self, command: MintLease) -> bool {
        self.data == LeaseCommandBindingData::Mint(command)
    }
    pub(crate) open spec fn matches_acquire(&self, command: AcquireLease) -> bool {
        self.data == LeaseCommandBindingData::Acquire(command)
    }
    pub(crate) open spec fn matches_renew(&self, command: RenewLease) -> bool {
        match &self.data {
            LeaseCommandBindingData::Renew(actual) => **actual == command,
            _ => false,
        }
    }
    pub(crate) open spec fn matches_use(&self, command: &UseLease) -> bool {
        self.matches_use_lease_inputs(
            command.command_id,
            command.claim,
            command.observed_at,
        ) && self.matches_use_capability(&command.capability_use)
    }
    pub(crate) open spec fn matches_use_lease_inputs(
        &self,
        command_id: peritus_types::CommandId,
        claim: crate::LeaseClaim,
        observed_at: peritus_policy::AuthorityInstant,
    ) -> bool {
        match &self.data {
            LeaseCommandBindingData::Use(binding) => {
                binding.matches_lease_inputs(command_id, claim, observed_at)
            }
            _ => false,
        }
    }
    pub(crate) open spec fn matches_use_capability(
        &self,
        capability_use: &peritus_policy::CapabilityUseTransition,
    ) -> bool {
        match &self.data {
            LeaseCommandBindingData::Use(binding) => {
                binding.matches_capability_use(capability_use)
            }
            _ => false,
        }
    }
    pub(crate) open spec fn matches_use_transition(
        &self,
        transition: &crate::LeaseUseTransition,
    ) -> bool {
        self.matches_use_lease_inputs(
            transition.lease.record.command_id,
            transition.claim,
            transition.capability_use.spec_used_at(),
        ) && self.matches_use_capability(&transition.capability_use)
    }
    pub(crate) open spec fn matches_release(&self, command: ReleaseLease) -> bool {
        match &self.data {
            LeaseCommandBindingData::Release(actual) => **actual == command,
            _ => false,
        }
    }
    pub(crate) open spec fn matches_expire(&self, command: ExpireLease) -> bool {
        self.data == LeaseCommandBindingData::Expire(command)
    }
    pub(crate) open spec fn matches_holder_loss(&self, command: FenceHolderLoss) -> bool {
        match &self.data {
            LeaseCommandBindingData::HolderLoss(actual) => **actual == command,
            _ => false,
        }
    }
    pub(crate) open spec fn matches_clock_discontinuity(
        &self,
        command: FenceClockDiscontinuity,
    ) -> bool {
        self.data == LeaseCommandBindingData::ClockDiscontinuity(command)
    }
    pub(crate) open spec fn matches_revoke(&self, command: RevokeLease) -> bool {
        match &self.data {
            LeaseCommandBindingData::Revoke(actual) => **actual == command,
            _ => false,
        }
    }
    pub(crate) open spec fn matches_reconcile(&self, command: ReconcileLease) -> bool {
        match &self.data {
            LeaseCommandBindingData::Reconcile(actual) => **actual == command,
            _ => false,
        }
    }

    /// Returns the exact source-command family.
    #[must_use]
    pub const fn kind(&self) -> LeaseCommandBindingKind {
        match &self.data {
            LeaseCommandBindingData::Mint(_) => LeaseCommandBindingKind::Mint,
            LeaseCommandBindingData::Acquire(_) => LeaseCommandBindingKind::Acquire,
            LeaseCommandBindingData::Renew(_) => LeaseCommandBindingKind::Renew,
            LeaseCommandBindingData::Use(_) => LeaseCommandBindingKind::Use,
            LeaseCommandBindingData::Release(_) => LeaseCommandBindingKind::Release,
            LeaseCommandBindingData::Expire(_) => LeaseCommandBindingKind::Expire,
            LeaseCommandBindingData::HolderLoss(_) => LeaseCommandBindingKind::HolderLoss,
            LeaseCommandBindingData::ClockDiscontinuity(_) => {
                LeaseCommandBindingKind::ClockDiscontinuity
            }
            LeaseCommandBindingData::Revoke(_) => LeaseCommandBindingKind::Revoke,
            LeaseCommandBindingData::Reconcile(_) => LeaseCommandBindingKind::Reconcile,
        }
    }

    /// Returns the exact mint command when this is a mint binding.
    #[must_use]
    pub const fn as_mint(&self) -> Option<MintLease> {
        if let LeaseCommandBindingData::Mint(command) = &self.data { Some(*command) } else { None }
    }
    /// Returns the exact acquire command when this is an acquire binding.
    #[must_use]
    pub const fn as_acquire(&self) -> Option<AcquireLease> {
        if let LeaseCommandBindingData::Acquire(command) = &self.data { Some(*command) } else { None }
    }
    /// Returns the exact renewal command when this is a renewal binding.
    #[must_use]
    pub const fn as_renew(&self) -> Option<RenewLease> {
        if let LeaseCommandBindingData::Renew(command) = &self.data { Some(**command) } else { None }
    }
    /// Returns the exact use projection when this is a policy-use binding.
    #[must_use]
    pub fn as_use(&self) -> Option<&LeaseUseCommandBinding> {
        if let LeaseCommandBindingData::Use(binding) = &self.data { Some(binding) } else { None }
    }
    /// Returns the exact release command when this is a release binding.
    #[must_use]
    pub const fn as_release(&self) -> Option<ReleaseLease> {
        if let LeaseCommandBindingData::Release(command) = &self.data { Some(**command) } else { None }
    }
    /// Returns the exact expiry command when this is an expiry binding.
    #[must_use]
    pub const fn as_expire(&self) -> Option<ExpireLease> {
        if let LeaseCommandBindingData::Expire(command) = &self.data { Some(*command) } else { None }
    }
    /// Returns the exact holder-loss command when this is a holder-loss binding.
    #[must_use]
    pub const fn as_holder_loss(&self) -> Option<FenceHolderLoss> {
        if let LeaseCommandBindingData::HolderLoss(command) = &self.data { Some(**command) } else { None }
    }
    /// Returns the exact discontinuity command when this is a discontinuity binding.
    #[must_use]
    pub const fn as_clock_discontinuity(&self) -> Option<FenceClockDiscontinuity> {
        if let LeaseCommandBindingData::ClockDiscontinuity(command) = &self.data {
            Some(*command)
        } else {
            None
        }
    }
    /// Returns the exact revocation command when this is a revocation binding.
    #[must_use]
    pub const fn as_revoke(&self) -> Option<RevokeLease> {
        if let LeaseCommandBindingData::Revoke(command) = &self.data { Some(**command) } else { None }
    }
    /// Returns the exact reconciliation command when this is a reconciliation binding.
    #[must_use]
    pub const fn as_reconcile(&self) -> Option<ReconcileLease> {
        if let LeaseCommandBindingData::Reconcile(command) = &self.data { Some(**command) } else { None }
    }
}

} // verus!
