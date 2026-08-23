//! Exact equality for the closed lease command-binding family.

mod commands;
mod use_projection;

use crate::binding::LeaseCommandBindingData;
use crate::LeaseCommandBinding;
use vstd::prelude::*;

verus! {

pub(crate) open spec fn binding_fields_match(
    left: &LeaseCommandBinding,
    right: &LeaseCommandBinding,
) -> bool {
    match (&left.data, &right.data) {
        (LeaseCommandBindingData::Mint(left), LeaseCommandBindingData::Mint(right)) => {
            commands::mint_fields_match(*left, *right)
        }
        (LeaseCommandBindingData::Acquire(left), LeaseCommandBindingData::Acquire(right)) => {
            commands::acquire_fields_match(*left, *right)
        }
        (LeaseCommandBindingData::Renew(left), LeaseCommandBindingData::Renew(right)) => {
            commands::renew_fields_match(**left, **right)
        }
        (LeaseCommandBindingData::Use(left), LeaseCommandBindingData::Use(right)) => {
            use_projection::use_binding_fields_match(left, right)
        }
        (LeaseCommandBindingData::Release(left), LeaseCommandBindingData::Release(right)) => {
            commands::release_fields_match(**left, **right)
        }
        (LeaseCommandBindingData::Expire(left), LeaseCommandBindingData::Expire(right)) => {
            commands::expire_fields_match(*left, *right)
        }
        (
            LeaseCommandBindingData::HolderLoss(left),
            LeaseCommandBindingData::HolderLoss(right),
        ) => commands::holder_loss_fields_match(**left, **right),
        (
            LeaseCommandBindingData::ClockDiscontinuity(left),
            LeaseCommandBindingData::ClockDiscontinuity(right),
        ) => commands::discontinuity_fields_match(*left, *right),
        (LeaseCommandBindingData::Revoke(left), LeaseCommandBindingData::Revoke(right)) => {
            commands::revoke_fields_match(**left, **right)
        }
        (
            LeaseCommandBindingData::Reconcile(left),
            LeaseCommandBindingData::Reconcile(right),
        ) => commands::reconcile_fields_match(**left, **right),
        _ => false,
    }
}

pub(super) fn bindings_equal(
    left: &LeaseCommandBinding,
    right: &LeaseCommandBinding,
) -> (equal: bool)
    ensures equal == binding_fields_match(left, right),
{
    match (&left.data, &right.data) {
        (LeaseCommandBindingData::Mint(left), LeaseCommandBindingData::Mint(right)) => {
            commands::mints_equal(*left, *right)
        }
        (LeaseCommandBindingData::Acquire(left), LeaseCommandBindingData::Acquire(right)) => {
            commands::acquires_equal(*left, *right)
        }
        (LeaseCommandBindingData::Renew(left), LeaseCommandBindingData::Renew(right)) => {
            commands::renews_equal(**left, **right)
        }
        (LeaseCommandBindingData::Use(left), LeaseCommandBindingData::Use(right)) => {
            use_projection::use_bindings_equal(left, right)
        }
        (LeaseCommandBindingData::Release(left), LeaseCommandBindingData::Release(right)) => {
            commands::releases_equal(left, right)
        }
        (LeaseCommandBindingData::Expire(left), LeaseCommandBindingData::Expire(right)) => {
            commands::expires_equal(*left, *right)
        }
        (
            LeaseCommandBindingData::HolderLoss(left),
            LeaseCommandBindingData::HolderLoss(right),
        ) => commands::holder_losses_equal(**left, **right),
        (
            LeaseCommandBindingData::ClockDiscontinuity(left),
            LeaseCommandBindingData::ClockDiscontinuity(right),
        ) => commands::discontinuities_equal(*left, *right),
        (LeaseCommandBindingData::Revoke(left), LeaseCommandBindingData::Revoke(right)) => {
            commands::revokes_equal(**left, **right)
        }
        (
            LeaseCommandBindingData::Reconcile(left),
            LeaseCommandBindingData::Reconcile(right),
        ) => commands::reconciles_equal(**left, **right),
        _ => false,
    }
}

} // verus!
