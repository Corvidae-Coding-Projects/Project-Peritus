//! Closed command-family binding carried by transition records.
#![allow(
    missing_docs,
    reason = "pinned Cargo-Verus synthesizes undocumented accessors for documented payload variants"
)]

use super::LeaseUseCommandBinding;
use crate::{
    AcquireLease, ExpireLease, FenceClockDiscontinuity, FenceHolderLoss, MintLease, ReconcileLease,
    ReleaseLease, RenewLease, RevokeLease, UseLease,
};

#[doc = " Stable tag for the exact source command projected into an accepted lease plan."]
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
#[doc = " Exact, unprivileged source-command binding carried by every accepted transition and CAS echo."]
#[doc = ""]
#[doc = " Construction is private to accepted reducers. Public projections are immutable evidence only;"]
#[doc = " they cannot create a lease, durable receipt, holder handle, or effect permit."]
#[derive(Debug, Eq, Hash, PartialEq)]
pub struct LeaseCommandBinding {
    pub(crate) data: LeaseCommandBindingData,
}
impl LeaseCommandBinding {
    pub(crate) const fn mint(command: MintLease) -> Self {
        Self { data: LeaseCommandBindingData::Mint(command) }
    }
    pub(crate) const fn acquire(command: AcquireLease) -> Self {
        Self { data: LeaseCommandBindingData::Acquire(command) }
    }
    pub(crate) fn renew(command: RenewLease) -> Self {
        Self { data: LeaseCommandBindingData::Renew(Box::new(command)) }
    }
    pub(crate) fn use_command(command: &UseLease) -> Self {
        Self {
            data: LeaseCommandBindingData::Use(Box::new(LeaseUseCommandBinding::from_command(
                command,
            ))),
        }
    }
    pub(crate) fn release(command: &ReleaseLease) -> Self {
        Self { data: LeaseCommandBindingData::Release(Box::new(*command)) }
    }
    pub(crate) const fn expire(command: ExpireLease) -> Self {
        Self { data: LeaseCommandBindingData::Expire(command) }
    }
    pub(crate) fn holder_loss(command: FenceHolderLoss) -> Self {
        Self { data: LeaseCommandBindingData::HolderLoss(Box::new(command)) }
    }
    pub(crate) const fn clock_discontinuity(command: FenceClockDiscontinuity) -> Self {
        Self { data: LeaseCommandBindingData::ClockDiscontinuity(command) }
    }
    pub(crate) fn revoke(command: RevokeLease) -> Self {
        Self { data: LeaseCommandBindingData::Revoke(Box::new(command)) }
    }
    pub(crate) fn reconcile(command: &ReconcileLease) -> Self {
        Self { data: LeaseCommandBindingData::Reconcile(Box::new(*command)) }
    }
    #[doc = " Returns the exact source-command family."]
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
    #[doc = " Returns the exact mint command when this is a mint binding."]
    #[must_use]
    pub const fn as_mint(&self) -> Option<MintLease> {
        if let LeaseCommandBindingData::Mint(command) = &self.data { Some(*command) } else { None }
    }
    #[doc = " Returns the exact acquire command when this is an acquire binding."]
    #[must_use]
    pub const fn as_acquire(&self) -> Option<AcquireLease> {
        if let LeaseCommandBindingData::Acquire(command) = &self.data {
            Some(*command)
        } else {
            None
        }
    }
    #[doc = " Returns the exact renewal command when this is a renewal binding."]
    #[must_use]
    pub const fn as_renew(&self) -> Option<RenewLease> {
        if let LeaseCommandBindingData::Renew(command) = &self.data {
            Some(**command)
        } else {
            None
        }
    }
    #[doc = " Returns the exact use projection when this is a policy-use binding."]
    #[must_use]
    pub fn as_use(&self) -> Option<&LeaseUseCommandBinding> {
        if let LeaseCommandBindingData::Use(binding) = &self.data { Some(binding) } else { None }
    }
    #[doc = " Returns the exact release command when this is a release binding."]
    #[must_use]
    pub const fn as_release(&self) -> Option<ReleaseLease> {
        if let LeaseCommandBindingData::Release(command) = &self.data {
            Some(**command)
        } else {
            None
        }
    }
    #[doc = " Returns the exact expiry command when this is an expiry binding."]
    #[must_use]
    pub const fn as_expire(&self) -> Option<ExpireLease> {
        if let LeaseCommandBindingData::Expire(command) = &self.data {
            Some(*command)
        } else {
            None
        }
    }
    #[doc = " Returns the exact holder-loss command when this is a holder-loss binding."]
    #[must_use]
    pub const fn as_holder_loss(&self) -> Option<FenceHolderLoss> {
        if let LeaseCommandBindingData::HolderLoss(command) = &self.data {
            Some(**command)
        } else {
            None
        }
    }
    #[doc = " Returns the exact discontinuity command when this is a discontinuity binding."]
    #[must_use]
    pub const fn as_clock_discontinuity(&self) -> Option<FenceClockDiscontinuity> {
        if let LeaseCommandBindingData::ClockDiscontinuity(command) = &self.data {
            Some(*command)
        } else {
            None
        }
    }
    #[doc = " Returns the exact revocation command when this is a revocation binding."]
    #[must_use]
    pub const fn as_revoke(&self) -> Option<RevokeLease> {
        if let LeaseCommandBindingData::Revoke(command) = &self.data {
            Some(**command)
        } else {
            None
        }
    }
    #[doc = " Returns the exact reconciliation command when this is a reconciliation binding."]
    #[must_use]
    pub const fn as_reconcile(&self) -> Option<ReconcileLease> {
        if let LeaseCommandBindingData::Reconcile(command) = &self.data {
            Some(**command)
        } else {
            None
        }
    }
}
