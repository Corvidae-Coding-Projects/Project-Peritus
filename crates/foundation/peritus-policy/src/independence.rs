//! Canonical approver-independence conjunction values.

use crate::{CanonicalCollection, PolicyError};
use core::cmp::Ordering;

#[doc = " Independence constraint imposed on an approver."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IndependenceRequirement {
    #[doc = " The requester cannot approve the request."]
    NotRequester,
    #[doc = " The action actor cannot approve the request."]
    NotActionActor,
    #[doc = " A participant in the producing attempt cannot approve it."]
    NoProducingAttemptParticipation,
    #[doc = " A participant in the candidate review cannot approve its waiver or acceptance."]
    NoReviewParticipation,
}
impl IndependenceRequirement {
    const fn rank(self) -> u8 {
        match self {
            Self::NotRequester => 0,
            Self::NotActionActor => 1,
            Self::NoProducingAttemptParticipation => 2,
            Self::NoReviewParticipation => 3,
        }
    }
}
#[doc = " Canonical duplicate-free set of approver independence requirements."]
#[derive(Debug, Eq, PartialEq)]
pub struct IndependenceSet {
    values: Vec<IndependenceRequirement>,
}
impl IndependenceSet {
    #[doc = " Validates requirements in strict canonical order. Empty sets are valid."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a precise duplicate or ordering failure."]
    pub fn new(values: Vec<IndependenceRequirement>) -> Result<Self, PolicyError> {
        let mut index = 1;
        while index < values.len() {
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
        Ok(requirements)
    }
    #[doc = " Borrows requirements in canonical order."]
    #[must_use]
    pub const fn as_slice(&self) -> &[IndependenceRequirement] {
        self.values.as_slice()
    }
    pub(crate) fn union(&self, other: &Self) -> Self {
        let mut values = Vec::new();
        let mut left = 0;
        let mut right = 0;
        while left < self.values.len() || right < other.values.len() {
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
    pub(crate) fn duplicate(&self) -> Self {
        let mut values = Vec::new();
        let mut index = 0;
        while index < self.values.len() {
            values.push(self.values[index]);
            index += 1;
        }
        Self { values }
    }
}
