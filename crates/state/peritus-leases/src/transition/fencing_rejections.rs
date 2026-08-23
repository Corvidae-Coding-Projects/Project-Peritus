//! Exact rejection constructors for command-specific fencing decision contracts.

use super::{rejection, LeaseAggregate, LeaseError, LeaseTransitionOutcome};
use crate::{
    ExpireLease, FenceClockDiscontinuity, FenceHolderLoss, ReleaseLease, RevokeLease,
};
use vstd::prelude::*;

verus! {

pub(super) const fn release(
    aggregate: LeaseAggregate,
    _command: &ReleaseLease,
    error: LeaseError,
) -> (result: LeaseTransitionOutcome)
    requires crate::model::concrete::rejections::fencing::release::release_error(
        &aggregate, *_command,
    ) == Some(error),
    ensures crate::model::concrete::rejections::fencing::release::concrete_release_decision(
        &aggregate, result, *_command,
    ),
{
    let ghost before = aggregate;
    let failure = rejection(aggregate, error);
    proof {
        crate::model::concrete::rejections::fencing::release::establish_release_rejection(
            &before, &failure, *_command, error,
        );
    }
    LeaseTransitionOutcome::Rejected(failure)
}

pub(super) const fn expiry(
    aggregate: LeaseAggregate,
    _command: ExpireLease,
    error: LeaseError,
) -> (result: LeaseTransitionOutcome)
    requires crate::model::concrete::rejections::fencing::expiry::expiry_error(
        &aggregate, _command,
    ) == Some(error),
    ensures crate::model::concrete::rejections::fencing::expiry::concrete_expiry_decision(
        &aggregate, result, _command,
    ),
{
    let ghost before = aggregate;
    let failure = rejection(aggregate, error);
    proof {
        crate::model::concrete::rejections::fencing::expiry::establish_expiry_rejection(
            &before, &failure, _command, error,
        );
    }
    LeaseTransitionOutcome::Rejected(failure)
}

pub(super) const fn holder_loss(
    aggregate: LeaseAggregate,
    _command: FenceHolderLoss,
    error: LeaseError,
) -> (result: LeaseTransitionOutcome)
    requires crate::model::concrete::rejections::fencing::holder_loss::holder_loss_error(
        &aggregate, _command,
    ) == Some(error),
    ensures crate::model::concrete::rejections::fencing::holder_loss::concrete_holder_loss_decision(
        &aggregate, result, _command,
    ),
{
    let ghost before = aggregate;
    let failure = rejection(aggregate, error);
    proof {
        crate::model::concrete::rejections::fencing::holder_loss::establish_holder_loss_rejection(
            &before, &failure, _command, error,
        );
    }
    LeaseTransitionOutcome::Rejected(failure)
}

pub(super) const fn discontinuity(
    aggregate: LeaseAggregate,
    _command: FenceClockDiscontinuity,
    error: LeaseError,
) -> (result: LeaseTransitionOutcome)
    requires crate::model::concrete::rejections::fencing::discontinuity::discontinuity_error(
        &aggregate, _command,
    ) == Some(error),
    ensures crate::model::concrete::rejections::fencing::discontinuity::concrete_discontinuity_decision(
        &aggregate, result, _command,
    ),
{
    let ghost before = aggregate;
    let failure = rejection(aggregate, error);
    proof {
        crate::model::concrete::rejections::fencing::discontinuity::establish_discontinuity_rejection(
            &before, &failure, _command, error,
        );
    }
    LeaseTransitionOutcome::Rejected(failure)
}

pub(super) const fn revoke(
    aggregate: LeaseAggregate,
    _command: RevokeLease,
    error: LeaseError,
) -> (result: LeaseTransitionOutcome)
    requires crate::model::concrete::rejections::fencing::revoke::revoke_error(
        &aggregate, _command,
    ) == Some(error),
    ensures crate::model::concrete::rejections::fencing::revoke::concrete_revoke_decision(
        &aggregate, result, _command,
    ),
{
    let ghost before = aggregate;
    let failure = rejection(aggregate, error);
    proof {
        crate::model::concrete::rejections::fencing::revoke::establish_revoke_rejection(
            &before, &failure, _command, error,
        );
    }
    LeaseTransitionOutcome::Rejected(failure)
}

} // verus!
