//! Constructive exact approval-conjunction model.

#![cfg(verus_only)]

use crate::{
    ActorRole, ApprovalRequirement, AuthorityTier, IndependenceRequirement, ValidityWindow,
};
use vstd::prelude::*;

verus! {

/// Returns the stricter of two authenticated approval authority tiers.
pub open spec fn maximum_authority_tier(
    left: AuthorityTier,
    right: AuthorityTier,
) -> AuthorityTier {
    if left.spec_rank() >= right.spec_rank() { left } else { right }
}

/// Exact canonical intersection of two approver-role suffixes.
pub open spec fn role_intersection_from(
    left: Seq<ActorRole>,
    right: Seq<ActorRole>,
    left_index: nat,
    right_index: nat,
) -> Seq<ActorRole>
    decreases (left.len() - left_index) + (right.len() - right_index),
{
    if left_index >= left.len() || right_index >= right.len() {
        Seq::empty()
    } else if left[left_index as int].spec_rank() < right[right_index as int].spec_rank() {
        role_intersection_from(left, right, left_index + 1, right_index)
    } else if left[left_index as int].spec_rank() > right[right_index as int].spec_rank() {
        role_intersection_from(left, right, left_index, right_index + 1)
    } else {
        Seq::empty().push(left[left_index as int])
            + role_intersection_from(left, right, left_index + 1, right_index + 1)
    }
}

/// Exact canonical union of two independence-requirement suffixes.
pub open spec fn independence_union_from(
    left: Seq<IndependenceRequirement>,
    right: Seq<IndependenceRequirement>,
    left_index: nat,
    right_index: nat,
) -> Seq<IndependenceRequirement>
    decreases (left.len() - left_index) + (right.len() - right_index),
{
    if left_index >= left.len() {
        right.subrange(right_index as int, right.len() as int)
    } else if right_index >= right.len() {
        left.subrange(left_index as int, left.len() as int)
    } else if left[left_index as int].spec_rank()
        < right[right_index as int].spec_rank()
    {
        Seq::empty().push(left[left_index as int])
            + independence_union_from(left, right, left_index + 1, right_index)
    } else if left[left_index as int].spec_rank()
        > right[right_index as int].spec_rank()
    {
        Seq::empty().push(right[right_index as int])
            + independence_union_from(left, right, left_index, right_index + 1)
    } else {
        Seq::empty().push(left[left_index as int])
            + independence_union_from(left, right, left_index + 1, right_index + 1)
    }
}

/// Returns the exact reason two runtime validity windows cannot form a nonempty intersection.
pub open spec fn window_intersection_conflict(
    left: ValidityWindow,
    right: ValidityWindow,
) -> bool {
    let not_before_epoch = if left.spec_not_before().spec_tick_millis()
        >= right.spec_not_before().spec_tick_millis()
    {
        left.spec_not_before().spec_epoch()
    } else {
        right.spec_not_before().spec_epoch()
    };
    let expires_epoch = if left.spec_expires_at().spec_tick_millis()
        <= right.spec_expires_at().spec_tick_millis()
    {
        left.spec_expires_at().spec_epoch()
    } else {
        right.spec_expires_at().spec_epoch()
    };
    left.spec_not_before().spec_epoch() != right.spec_not_before().spec_epoch()
        || not_before_epoch != expires_epoch
        || crate::model::maximum_int(
            left.spec_not_before().spec_tick_millis(),
            right.spec_not_before().spec_tick_millis(),
        ) >= crate::model::minimum_int(
            left.spec_expires_at().spec_tick_millis(),
            right.spec_expires_at().spec_tick_millis(),
        )
}

/// Returns the exact epoch selected for an intersection's inclusive bound.
pub open spec fn intersection_not_before_epoch(
    left: ValidityWindow,
    right: ValidityWindow,
) -> int {
    if left.spec_not_before().spec_tick_millis()
        >= right.spec_not_before().spec_tick_millis()
    {
        left.spec_not_before().spec_epoch()
    } else {
        right.spec_not_before().spec_epoch()
    }
}

/// Returns the exact epoch selected for an intersection's exclusive bound.
pub open spec fn intersection_expires_epoch(
    left: ValidityWindow,
    right: ValidityWindow,
) -> int {
    if left.spec_expires_at().spec_tick_millis()
        <= right.spec_expires_at().spec_tick_millis()
    {
        left.spec_expires_at().spec_epoch()
    } else {
        right.spec_expires_at().spec_epoch()
    }
}

/// Exact conjunction semantics for two approval requirements and their typed result.
pub open spec fn approval_conjunction_result(
    left: &ApprovalRequirement,
    right: &ApprovalRequirement,
    result: &Option<ApprovalRequirement>,
) -> bool {
    let roles = role_intersection_from(
        left.spec_approver_roles(),
        right.spec_approver_roles(),
        0,
        0,
    );
    match result {
        Some(requirement) => {
            roles.len() > 0
                && !window_intersection_conflict(
                    left.spec_validity(),
                    right.spec_validity(),
                )
                && requirement.spec_minimum_tier()
                    == maximum_authority_tier(
                        left.spec_minimum_tier(),
                        right.spec_minimum_tier(),
                    )
                && requirement.spec_approver_roles() == roles
                && requirement.spec_independence()
                    == independence_union_from(
                        left.spec_independence(),
                        right.spec_independence(),
                        0,
                        0,
                    )
                && requirement.spec_validity().spec_not_before().spec_epoch()
                    == intersection_not_before_epoch(
                        left.spec_validity(),
                        right.spec_validity(),
                    )
                && requirement.spec_validity().spec_not_before().spec_tick_millis()
                    == crate::model::maximum_int(
                        left.spec_validity().spec_not_before().spec_tick_millis(),
                        right.spec_validity().spec_not_before().spec_tick_millis(),
                    )
                && requirement.spec_validity().spec_expires_at().spec_tick_millis()
                    == crate::model::minimum_int(
                        left.spec_validity().spec_expires_at().spec_tick_millis(),
                        right.spec_validity().spec_expires_at().spec_tick_millis(),
                    )
                && requirement.spec_validity().spec_expires_at().spec_epoch()
                    == intersection_expires_epoch(
                        left.spec_validity(),
                        right.spec_validity(),
                    )
        }
        None => {
            roles.len() == 0
                || window_intersection_conflict(left.spec_validity(), right.spec_validity())
        }
    }
}

/// Exact validity-only refinement of one accumulated approval requirement.
pub open spec fn constrained_approval_result(
    original: &ApprovalRequirement,
    constraint: ValidityWindow,
    result: &Option<ApprovalRequirement>,
) -> bool {
    match result {
        Some(requirement) => {
            !window_intersection_conflict(original.spec_validity(), constraint)
                && requirement.spec_minimum_tier() == original.spec_minimum_tier()
                && requirement.spec_approver_roles() == original.spec_approver_roles()
                && requirement.spec_independence() == original.spec_independence()
                && requirement.spec_validity().spec_not_before().spec_epoch()
                    == intersection_not_before_epoch(original.spec_validity(), constraint)
                && requirement.spec_validity().spec_not_before().spec_tick_millis()
                    == crate::model::maximum_int(
                        original.spec_validity().spec_not_before().spec_tick_millis(),
                        constraint.spec_not_before().spec_tick_millis(),
                    )
                && requirement.spec_validity().spec_expires_at().spec_tick_millis()
                    == crate::model::minimum_int(
                        original.spec_validity().spec_expires_at().spec_tick_millis(),
                        constraint.spec_expires_at().spec_tick_millis(),
                    )
                && requirement.spec_validity().spec_expires_at().spec_epoch()
                    == intersection_expires_epoch(original.spec_validity(), constraint)
        }
        None => window_intersection_conflict(original.spec_validity(), constraint),
    }
}

} // verus!
