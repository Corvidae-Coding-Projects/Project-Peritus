//! Exact equality for the closed lease command-binding family.

mod commands;
mod use_projection;

use crate::LeaseCommandBinding;
use crate::binding::LeaseCommandBindingData;

pub(super) fn bindings_equal(left: &LeaseCommandBinding, right: &LeaseCommandBinding) -> bool {
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
        (LeaseCommandBindingData::HolderLoss(left), LeaseCommandBindingData::HolderLoss(right)) => {
            commands::holder_losses_equal(**left, **right)
        }
        (
            LeaseCommandBindingData::ClockDiscontinuity(left),
            LeaseCommandBindingData::ClockDiscontinuity(right),
        ) => commands::discontinuities_equal(*left, *right),
        (LeaseCommandBindingData::Revoke(left), LeaseCommandBindingData::Revoke(right)) => {
            commands::revokes_equal(**left, **right)
        }
        (LeaseCommandBindingData::Reconcile(left), LeaseCommandBindingData::Reconcile(right)) => {
            commands::reconciles_equal(**left, **right)
        }
        _ => false,
    }
}
