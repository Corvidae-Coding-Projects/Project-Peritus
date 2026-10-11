//! Exact rejection constructors for command-specific fencing decision contracts.

use super::{LeaseAggregate, LeaseError, LeaseTransitionOutcome, rejection};
use crate::{ExpireLease, FenceClockDiscontinuity, FenceHolderLoss, ReleaseLease, RevokeLease};

pub(super) const fn release(
    aggregate: LeaseAggregate,
    _command: &ReleaseLease,
    error: LeaseError,
) -> LeaseTransitionOutcome {
    let failure = rejection(aggregate, error);
    LeaseTransitionOutcome::Rejected(failure)
}
pub(super) const fn expiry(
    aggregate: LeaseAggregate,
    _command: ExpireLease,
    error: LeaseError,
) -> LeaseTransitionOutcome {
    let failure = rejection(aggregate, error);
    LeaseTransitionOutcome::Rejected(failure)
}
pub(super) const fn holder_loss(
    aggregate: LeaseAggregate,
    _command: FenceHolderLoss,
    error: LeaseError,
) -> LeaseTransitionOutcome {
    let failure = rejection(aggregate, error);
    LeaseTransitionOutcome::Rejected(failure)
}
pub(super) const fn discontinuity(
    aggregate: LeaseAggregate,
    _command: FenceClockDiscontinuity,
    error: LeaseError,
) -> LeaseTransitionOutcome {
    let failure = rejection(aggregate, error);
    LeaseTransitionOutcome::Rejected(failure)
}
pub(super) const fn revoke(
    aggregate: LeaseAggregate,
    _command: RevokeLease,
    error: LeaseError,
) -> LeaseTransitionOutcome {
    let failure = rejection(aggregate, error);
    LeaseTransitionOutcome::Rejected(failure)
}
