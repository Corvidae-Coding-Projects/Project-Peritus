//! Canonical bounded participant-set construction and membership.

use core::cmp::Ordering;
use peritus_types::ActorId;

const fn compare_actor_bytes_from(left: &[u8; 16], right: &[u8; 16], index: usize) -> Ordering {
    if index == 16 {
        Ordering::Equal
    } else if left[index] < right[index] {
        Ordering::Less
    } else if left[index] > right[index] {
        Ordering::Greater
    } else {
        compare_actor_bytes_from(left, right, index + 1)
    }
}
const fn compare_actor_bytes(left: &ActorId, right: &ActorId) -> Ordering {
    compare_actor_bytes_from(left.as_bytes(), right.as_bytes(), 0)
}
impl super::ParticipantSet {
    fn checked(
        values: Vec<ActorId>,
        collection: crate::CanonicalCollection,
        maximum: usize,
    ) -> Result<Self, crate::ApprovalError> {
        if values.len() > maximum {
            return Err(crate::ApprovalError::CollectionTooLarge(collection));
        }
        let mut index = 0;
        while index < values.len() {
            if index > 0 {
                match compare_actor_bytes(&values[index - 1], &values[index]) {
                    Ordering::Less => {}
                    Ordering::Equal => {
                        return Err(crate::ApprovalError::DuplicateCanonicalValue(collection));
                    }
                    Ordering::Greater => {
                        return Err(crate::ApprovalError::NonCanonicalOrder(collection));
                    }
                }
            }
            index += 1;
        }
        Ok(Self { values })
    }
    #[doc = " Validates producing-attempt participants in strict actor-byte order."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects over-limit, duplicate, or noncanonically ordered actor IDs."]
    pub fn producing(values: Vec<ActorId>) -> Result<Self, crate::ApprovalError> {
        Self::checked(
            values,
            crate::CanonicalCollection::ProducingParticipants,
            super::MAX_PRODUCING_PARTICIPANTS,
        )
    }
    #[doc = " Validates review participants in strict actor-byte order."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects over-limit, duplicate, or noncanonically ordered actor IDs."]
    pub fn review(values: Vec<ActorId>) -> Result<Self, crate::ApprovalError> {
        Self::checked(
            values,
            crate::CanonicalCollection::ReviewParticipants,
            super::MAX_REVIEW_PARTICIPANTS,
        )
    }
    #[doc = " Borrows canonical participant actors."]
    #[must_use]
    pub const fn as_slice(&self) -> &[ActorId] {
        self.values.as_slice()
    }
    #[doc = " Returns whether one actor occurs in the canonical set."]
    #[must_use]
    pub fn contains(&self, actor: ActorId) -> bool {
        let target = *actor.as_bytes();
        let mut index = 0;
        while index < self.values.len() {
            let candidate = *self.values[index].as_bytes();
            if crate::state::exact::identifier_bytes_equal(candidate, target) {
                return true;
            }
            index += 1;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(bytes: [u8; 16]) -> ActorId {
        ActorId::new(bytes).expect("all fixed-width actor bytes are valid")
    }

    fn indexed_actor(index: usize) -> ActorId {
        let mut bytes = [0; 16];
        let encoded = u16::try_from(index + 1).expect("test actor index fits in two bytes");
        bytes[..2].copy_from_slice(&encoded.to_be_bytes());
        actor(bytes)
    }

    #[test]
    fn canonical_order_observes_every_actor_byte() {
        for differing_index in 0..16 {
            let mut left = [7; 16];
            let mut right = [7; 16];
            left[differing_index] = 1;
            right[differing_index] = 2;
            left[differing_index + 1..].fill(255);
            right[differing_index + 1..].fill(0);
            let left = actor(left);
            let right = actor(right);

            let accepted = super::super::ParticipantSet::producing(vec![left, right])
                .expect("first differing byte establishes canonical order");
            assert_eq!(accepted.as_slice(), &[left, right]);
            assert!(accepted.contains(left));
            assert!(accepted.contains(right));

            assert_eq!(
                super::super::ParticipantSet::review(vec![right, left]),
                Err(crate::ApprovalError::NonCanonicalOrder(
                    crate::CanonicalCollection::ReviewParticipants
                ))
            );
            assert_eq!(
                super::super::ParticipantSet::producing(vec![left, left]),
                Err(crate::ApprovalError::DuplicateCanonicalValue(
                    crate::CanonicalCollection::ProducingParticipants
                ))
            );
        }
    }

    #[test]
    fn participant_bounds_accept_exact_limit_and_reject_one_over() {
        let exact: Vec<_> =
            (0..super::super::MAX_PRODUCING_PARTICIPANTS).map(indexed_actor).collect();
        let over: Vec<_> = (0..=super::super::MAX_REVIEW_PARTICIPANTS).map(indexed_actor).collect();

        let accepted = super::super::ParticipantSet::producing(exact)
            .expect("the documented maximum is inclusive");
        assert_eq!(accepted.as_slice().len(), super::super::MAX_PRODUCING_PARTICIPANTS);
        assert_eq!(
            super::super::ParticipantSet::review(over),
            Err(crate::ApprovalError::CollectionTooLarge(
                crate::CanonicalCollection::ReviewParticipants
            ))
        );
    }

    #[test]
    fn empty_and_singleton_sets_have_exact_membership() {
        let empty = super::super::ParticipantSet::producing(Vec::new())
            .expect("empty provenance sets are valid");
        let only = indexed_actor(42);
        let singleton = super::super::ParticipantSet::review(vec![only])
            .expect("singleton provenance sets are valid");

        assert!(empty.as_slice().is_empty());
        assert!(!empty.contains(only));
        assert!(singleton.contains(only));
        assert!(!singleton.contains(indexed_actor(43)));
    }
}
