//! Closed total exact relations for cancellation and expiry reducers.

#[cfg(verus_only)]
use peritus_policy::AuthorityInstant;
use vstd::prelude::*;

verus! {

pub(super) open spec fn pending_expiry_result(
    request: &crate::ApprovalRequest,
) -> Result<AuthorityInstant, crate::ApprovalError> {
    let request_expiry = request.validity.spec_expires_at();
    let scope_expiry = request.scope.spec_validity().spec_expires_at();
    let requirement_expiry = request.requirement.spec_validity().spec_expires_at();
    if request_expiry.spec_epoch() != scope_expiry.spec_epoch()
        || request_expiry.spec_epoch() != requirement_expiry.spec_epoch()
    {
        Err(crate::ApprovalError::ClockEpochMismatch)
    } else if request_expiry.spec_tick_millis() <= scope_expiry.spec_tick_millis()
        && request_expiry.spec_tick_millis() <= requirement_expiry.spec_tick_millis()
    {
        Ok(request_expiry)
    } else if scope_expiry.spec_tick_millis() <= requirement_expiry.spec_tick_millis() {
        Ok(scope_expiry)
    } else {
        Ok(requirement_expiry)
    }
}

impl super::ApprovalAggregate {
    /// Closed total contract for cancellation, including exact rejection preservation.
    pub closed spec fn spec_cancel_result_is_exact(
        &self,
        result: &Result<super::ApprovalTransitionOutcome, crate::ApprovalTransitionFailure>,
    ) -> bool {
        cancel_result_relation(self, result)
    }

    /// Returns the exact ordered first failure for one expiry attempt.
    pub(super) open spec fn spec_expire_expected_error(
        &self,
        observed_at: AuthorityInstant,
    ) -> Option<crate::ApprovalError> {
        match self.state {
            super::ApprovalState::Pending => expire_error_from_bound(
                &self.request,
                observed_at,
                pending_expiry_result(&self.request),
            ),
            super::ApprovalState::ApprovedOnce(resolution)
            | super::ApprovalState::AmendmentAuthorized(resolution) => {
                expire_error_from_bound(&self.request, observed_at, Ok(resolution.valid_until))
            }
            _ => Some(crate::ApprovalError::IllegalPhase {
                expected: crate::ApprovalPhase::Pending,
                actual: self.spec_phase(),
            }),
        }
    }

    /// Returns the exact complete accepted expiry successor and transition record.
    pub(super) open spec fn spec_expire_success_is_exact(
        &self,
        observed_at: AuthorityInstant,
        outcome: &super::ApprovalTransitionOutcome,
    ) -> bool {
        match self.state {
            super::ApprovalState::Pending => {
                expire_success_for_resolution(self, observed_at, outcome, None)
            }
            super::ApprovalState::ApprovedOnce(value)
            | super::ApprovalState::AmendmentAuthorized(value) => {
                expire_success_for_resolution(self, observed_at, outcome, Some(value))
            }
            _ => false,
        }
    }

    /// Closed total contract for every expiry accepted or rejected result.
    pub closed spec fn spec_expire_result_is_exact(
        &self,
        observed_at: AuthorityInstant,
        result: &Result<super::ApprovalTransitionOutcome, crate::ApprovalTransitionFailure>,
    ) -> bool {
        expire_result_relation(self, observed_at, result)
    }
}

pub(super) open spec fn cancel_result_relation(
    before: &super::ApprovalAggregate,
    result: &Result<super::ApprovalTransitionOutcome, crate::ApprovalTransitionFailure>,
) -> bool {
    match before.state {
        super::ApprovalState::Pending => match result {
            Ok(outcome) => {
                outcome.aggregate.request == before.request
                    && outcome.aggregate.state == super::ApprovalState::Cancelled
                    && outcome.transition.kind == super::ApprovalTransitionKind::Cancelled
                    && outcome.transition.from == crate::ApprovalPhase::Pending
                    && outcome.transition.to == crate::ApprovalPhase::Cancelled
                    && outcome.transition.decision_digest.is_none()
                    && outcome.transition.registry_revision.is_none()
                    && crate::model::inv_009(outcome.spec_model())
            }
            Err(_) => false,
        },
        _ => match result {
            Err(failure) => {
                failure.error == crate::ApprovalError::IllegalPhase {
                    expected: crate::ApprovalPhase::Pending,
                    actual: before.spec_phase(),
                }
                    && failure.aggregate == *before
                    && failure.observation.is_none()
            }
            Ok(_) => false,
        },
    }
}

pub(super) proof fn close_cancel_relation(
    before: &super::ApprovalAggregate,
    result: &Result<super::ApprovalTransitionOutcome, crate::ApprovalTransitionFailure>,
)
    requires cancel_result_relation(before, result),
    ensures before.spec_cancel_result_is_exact(result),
{
}

pub(super) open spec fn expire_result_relation(
    before: &super::ApprovalAggregate,
    observed_at: AuthorityInstant,
    result: &Result<super::ApprovalTransitionOutcome, crate::ApprovalTransitionFailure>,
) -> bool {
    match before.spec_expire_expected_error(observed_at) {
        Some(expected) => match result {
            Err(failure) => {
                failure.error == expected
                    && failure.aggregate == *before
                    && failure.observation.is_none()
            }
            Ok(_) => false,
        },
        None => match result {
            Ok(outcome) => before.spec_expire_success_is_exact(observed_at, outcome),
            Err(_) => false,
        },
    }
}

pub(super) proof fn close_expire_relation(
    before: &super::ApprovalAggregate,
    observed_at: AuthorityInstant,
    result: &Result<super::ApprovalTransitionOutcome, crate::ApprovalTransitionFailure>,
)
    requires expire_result_relation(before, observed_at, result),
    ensures before.spec_expire_result_is_exact(observed_at, result),
{
}

pub(super) open spec fn expire_error_from_bound(
    request: &crate::ApprovalRequest,
    observed_at: AuthorityInstant,
    expiry_result: Result<AuthorityInstant, crate::ApprovalError>,
) -> Option<crate::ApprovalError> {
    match expiry_result {
        Err(error) => Some(error),
        Ok(expiry) => {
            if observed_at.spec_epoch() != expiry.spec_epoch() {
                Some(crate::ApprovalError::ClockEpochMismatch)
            } else if observed_at.spec_tick_millis() < expiry.spec_tick_millis() {
                Some(crate::ApprovalError::NotYetValid)
            } else {
                super::exact::observation_time_error(request, observed_at)
            }
        }
    }
}

pub(super) open spec fn expire_success_for_resolution(
    before: &super::ApprovalAggregate,
    observed_at: AuthorityInstant,
    outcome: &super::ApprovalTransitionOutcome,
    resolution: Option<super::Resolution>,
) -> bool {
    super::exact::request_is_exact_advance(
        &outcome.aggregate.request,
        &before.request,
        observed_at,
    )
        && outcome.aggregate.state == super::ApprovalState::Expired(resolution)
        && outcome.transition.kind == super::ApprovalTransitionKind::Expired
        && outcome.transition.from == before.spec_phase()
        && outcome.transition.to == crate::ApprovalPhase::Expired
        && outcome.transition.decision_digest == match resolution {
            Some(value) => Some(value.decision_digest),
            None => None,
        }
        && outcome.transition.registry_revision == match resolution {
            Some(value) => Some(value.registry_revision),
            None => None,
        }
        && crate::model::inv_009(outcome.spec_model())
}

} // verus!
