//! Canonical approver-independence conjunction values.

use crate::{CanonicalCollection, PolicyError};
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

/// Returns the first duplicate or descending adjacent independence requirement.
pub open spec fn first_independence_order_error(
    values: Seq<IndependenceRequirement>,
    index: nat,
) -> Option<crate::PolicyErrorKind>
    decreases values.len() - index,
{
    if index >= values.len() {
        None
    } else if values[index as int - 1].spec_rank() == values[index as int].spec_rank() {
        Some(crate::PolicyErrorKind::DuplicateCanonicalValue)
    } else if values[index as int - 1].spec_rank() > values[index as int].spec_rank() {
        Some(crate::PolicyErrorKind::NonCanonicalOrder)
    } else {
        first_independence_order_error(values, index + 1)
    }
}

/// Independence constraint imposed on an approver.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IndependenceRequirement {
    /// The requester cannot approve the request.
    NotRequester,
    /// The action actor cannot approve the request.
    NotActionActor,
    /// A participant in the producing attempt cannot approve it.
    NoProducingAttemptParticipation,
    /// A participant in the candidate review cannot approve its waiver or acceptance.
    NoReviewParticipation,
}

impl IndependenceRequirement {
    /// Returns the exact canonical independence rank used by specifications.
    pub open spec fn spec_rank(self) -> int {
        match self {
            Self::NotRequester => 0,
            Self::NotActionActor => 1,
            Self::NoProducingAttemptParticipation => 2,
            Self::NoReviewParticipation => 3,
        }
    }

    const fn rank(self) -> (rank: u8)
        ensures rank as int == self.spec_rank(),
    {
        match self {
            Self::NotRequester => 0,
            Self::NotActionActor => 1,
            Self::NoProducingAttemptParticipation => 2,
            Self::NoReviewParticipation => 3,
        }
    }
}

/// Canonical duplicate-free set of approver independence requirements.
#[derive(Debug, Eq, PartialEq)]
pub struct IndependenceSet {
    values: Vec<IndependenceRequirement>,
}

impl IndependenceSet {
    /// Returns the exact canonical requirement sequence used by specifications.
    pub closed spec fn spec_values(&self) -> Seq<IndependenceRequirement> { self.values@ }

    /// Validates requirements in strict canonical order. Empty sets are valid.
    ///
    /// # Errors
    ///
    /// Returns a precise duplicate or ordering failure.
    pub fn new(values: Vec<IndependenceRequirement>) -> (result: Result<Self, PolicyError>)
        ensures
            match result {
                Ok(requirements) => {
                    first_independence_order_error(values@, 1).is_none()
                        && requirements.spec_values() == values@
                }
                Err(error) => {
                    first_independence_order_error(values@, 1) == Some(error.spec_kind())
                        && error.spec_collection()
                            == Some(CanonicalCollection::IndependenceRequirements)
                        && error.spec_dimension().is_none()
                }
            },
    {
        let mut index = 1;
        while index < values.len()
            invariant
                (values.len() == 0 && index == 1) || 1 <= index <= values.len(),
                first_independence_order_error(values@, 1)
                    == first_independence_order_error(values@, index as nat),
            decreases values.len() - index,
        {
            let previous = values[index - 1].rank();
            let current = values[index].rank();
            if previous == current {
                return Err(PolicyError::duplicate_canonical_value(
                    CanonicalCollection::IndependenceRequirements,
                ));
            }
            if previous > current {
                return Err(PolicyError::non_canonical_order(
                    CanonicalCollection::IndependenceRequirements,
                ));
            }
            index += 1;
        }
        let requirements = Self { values };
        reveal(IndependenceSet::spec_values);
        Ok(requirements)
    }

    /// Borrows requirements in canonical order.
    #[must_use]
    pub const fn as_slice(&self) -> (requirements: &[IndependenceRequirement])
        ensures requirements@ == self.spec_values(),
    { self.values.as_slice() }

    pub(crate) fn union(&self, other: &Self) -> (union: Self)
        ensures
            union.spec_values() == crate::approval_model::independence_union_from(
                self.spec_values(),
                other.spec_values(),
                0,
                0,
            ),
    {
        let mut values = Vec::new();
        let mut left = 0;
        let mut right = 0;
        while left < self.values.len() || right < other.values.len()
            invariant
                0 <= left <= self.values.len(),
                0 <= right <= other.values.len(),
                values@ + crate::approval_model::independence_union_from(
                    self.values@,
                    other.values@,
                    left as nat,
                    right as nat,
                ) == crate::approval_model::independence_union_from(
                    self.values@,
                    other.values@,
                    0,
                    0,
                ),
            decreases (self.values.len() - left) + (other.values.len() - right),
        {
            if left == self.values.len() {
                values.push(other.values[right]);
                right += 1;
            } else if right == other.values.len() {
                values.push(self.values[left]);
                left += 1;
            } else {
                let left_rank = self.values[left].rank();
                let right_rank = other.values[right].rank();
                match left_rank.cmp(&right_rank) {
                    Ordering::Less => {
                        values.push(self.values[left]);
                        left += 1;
                    }
                    Ordering::Greater => {
                        values.push(other.values[right]);
                        right += 1;
                    }
                    Ordering::Equal => {
                        values.push(self.values[left]);
                        left += 1;
                        right += 1;
                    }
                }
            }
        }
        Self { values }
    }

    pub(crate) fn duplicate(&self) -> (duplicate: Self)
        ensures duplicate.spec_values() == self.spec_values(),
    {
        let mut values = Vec::new();
        let mut index = 0;
        while index < self.values.len()
            invariant
                0 <= index <= self.values.len(),
                values@ == self.values@.subrange(0, index as int),
            decreases self.values.len() - index,
        {
            values.push(self.values[index]);
            index += 1;
        }
        Self { values }
    }
}

} // verus!
