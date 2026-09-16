//! Independent review observations.

use crate::{ConstructionError, EvidenceBinding, PrincipalId, ReviewId};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Independent review outcome.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReviewOutcome {
    /// Reviewer approves the exact candidate for production evaluation.
    Approved,
    /// Reviewer requires changes before production evaluation.
    ChangesRequired,
}

/// One signed independent review observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReviewObservation {
    id: ReviewId,
    binding: EvidenceBinding,
    reviewer: PrincipalId,
    producer: PrincipalId,
    context_digest: Sha256Digest,
    review_digest: Sha256Digest,
    outcome: ReviewOutcome,
    independent_from_producer: bool,
}

impl ReviewObservation {
    /// Creates one signed review observation.
    ///
    /// # Errors
    ///
    /// Returns a typed error for a placeholder context or review digest.
    #[allow(clippy::too_many_arguments, reason = "review identity, independence, and outcome stay explicit")]
    pub fn new(
        id: ReviewId,
        binding: EvidenceBinding,
        reviewer: PrincipalId,
        producer: PrincipalId,
        context_digest: Sha256Digest,
        review_digest: Sha256Digest,
        outcome: ReviewOutcome,
        independent_from_producer: bool,
    ) -> (result: Result<Self, ConstructionError>)
        ensures
            result.is_ok() == (crate::validation::spec_digest_nonzero(context_digest)
                && crate::validation::spec_digest_nonzero(review_digest)),
            match result {
                Ok(value) => value.spec_id() == id
                    && value.spec_binding() == binding
                    && value.spec_reviewer() == reviewer
                    && value.spec_producer() == producer
                    && value.spec_context_digest() == context_digest
                    && value.spec_review_digest() == review_digest
                    && value.spec_outcome() == outcome
                    && value.spec_independent_from_producer() == independent_from_producer,
                Err(error) => error.spec_kind() == crate::ConstructionErrorKind::ZeroDigest,
            },
    {
        crate::validation::require_digest(context_digest)?;
        crate::validation::require_digest(review_digest)?;
        Ok(Self {
            id,
            binding,
            reviewer,
            producer,
            context_digest,
            review_digest,
            outcome,
            independent_from_producer,
        })
    }

    /// Returns the stable review identity.
    #[must_use]
    pub const fn id(&self) -> (id: ReviewId) ensures id == self.spec_id() { self.id }

    /// Returns the exact evidence binding.
    #[must_use]
    pub const fn binding(&self) -> (binding: EvidenceBinding)
        ensures binding == self.spec_binding()
    {
        self.binding
    }

    /// Returns the reviewer identity.
    #[must_use]
    pub const fn reviewer(&self) -> (reviewer: PrincipalId)
        ensures reviewer == self.spec_reviewer()
    {
        self.reviewer
    }

    /// Returns the candidate producer identity.
    #[must_use]
    pub const fn producer(&self) -> (producer: PrincipalId)
        ensures producer == self.spec_producer()
    {
        self.producer
    }

    /// Returns the fresh-context digest.
    #[must_use]
    pub const fn context_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_context_digest()
    {
        self.context_digest
    }

    /// Returns the signed review digest.
    #[must_use]
    pub const fn review_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_review_digest()
    {
        self.review_digest
    }

    /// Returns the explicit review outcome.
    #[must_use]
    pub const fn outcome(&self) -> (outcome: ReviewOutcome)
        ensures outcome == self.spec_outcome()
    {
        self.outcome
    }

    /// Returns the independently attested producer-separation status.
    #[must_use]
    pub const fn independent_from_producer(&self) -> (independent: bool)
        ensures independent == self.spec_independent_from_producer()
    {
        self.independent_from_producer
    }

    /// Logical view of the stable review identity.
    pub closed spec fn spec_id(&self) -> ReviewId { self.id }

    /// Logical view of the exact evidence binding.
    pub closed spec fn spec_binding(&self) -> EvidenceBinding { self.binding }

    /// Logical view of the reviewer identity.
    pub closed spec fn spec_reviewer(&self) -> PrincipalId { self.reviewer }

    /// Logical view of the candidate producer identity.
    pub closed spec fn spec_producer(&self) -> PrincipalId { self.producer }

    /// Logical view of the fresh-context digest.
    pub closed spec fn spec_context_digest(&self) -> Sha256Digest { self.context_digest }

    /// Logical view of the signed review digest.
    pub closed spec fn spec_review_digest(&self) -> Sha256Digest { self.review_digest }

    /// Logical view of the explicit review outcome.
    pub closed spec fn spec_outcome(&self) -> ReviewOutcome { self.outcome }

    /// Logical view of the independently attested producer separation.
    pub closed spec fn spec_independent_from_producer(&self) -> bool {
        self.independent_from_producer
    }

    /// Logical equality of every supplied review field.
    pub open spec fn spec_matches(&self, other: ReviewObservation) -> bool {
        crate::identity::review_ids_match(self.spec_id(), other.spec_id())
            && crate::evidence::bindings_match(self.spec_binding(), other.spec_binding())
            && crate::identity::principal_ids_match(
                self.spec_reviewer(),
                other.spec_reviewer(),
            )
            && crate::identity::principal_ids_match(
                self.spec_producer(),
                other.spec_producer(),
            )
            && crate::candidate::digest_matches(
                self.spec_context_digest(),
                other.spec_context_digest(),
            )
            && crate::candidate::digest_matches(
                self.spec_review_digest(),
                other.spec_review_digest(),
            )
            && self.spec_outcome() == other.spec_outcome()
            && self.spec_independent_from_producer()
                == other.spec_independent_from_producer()
    }
}

/// Executable equality of every supplied review field.
pub const fn reviews_equal(left: &ReviewObservation, right: &ReviewObservation) -> (equal: bool)
    ensures equal == left.spec_matches(*right),
{
    crate::identity::review_ids_equal(left.id(), right.id())
        && crate::evidence::bindings_equal(&left.binding(), &right.binding())
        && crate::identity::principal_ids_equal(left.reviewer(), right.reviewer())
        && crate::identity::principal_ids_equal(left.producer(), right.producer())
        && crate::candidate::equality::digests_equal(
            left.context_digest(),
            right.context_digest(),
        )
        && crate::candidate::equality::digests_equal(
            left.review_digest(),
            right.review_digest(),
        )
        && review_outcomes_equal(left.outcome(), right.outcome())
        && left.independent_from_producer() == right.independent_from_producer()
}

/// Executable equality for independent-review outcomes.
pub const fn review_outcomes_equal(left: ReviewOutcome, right: ReviewOutcome) -> (equal: bool)
    ensures equal == (left == right),
{
    matches!((left, right),
        (ReviewOutcome::Approved, ReviewOutcome::Approved)
            | (ReviewOutcome::ChangesRequired, ReviewOutcome::ChangesRequired))
}

} // verus!
