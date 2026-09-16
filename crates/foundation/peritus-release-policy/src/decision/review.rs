//! Aggregated independent-review assessment.

use vstd::prelude::*;

verus! {

/// Aggregated independent-review state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[allow(clippy::struct_excessive_bools, reason = "review independence dimensions must remain explicit")]
pub struct ReviewAssessment {
    satisfied: bool,
    approved_count: u16,
    stale_count: u16,
    mismatched_count: u16,
    changes_required_count: u16,
    self_review_count: u16,
    non_independent_count: u16,
    duplicate_reviewer: bool,
    shared_context: bool,
    conflicting_review: bool,
}

impl ReviewAssessment {
    #[allow(
        clippy::fn_params_excessive_bools,
        clippy::too_many_arguments,
        reason = "review independence dimensions remain explicit"
    )]
    pub(crate) const fn new(
        satisfied: bool,
        approved_count: u16,
        stale_count: u16,
        mismatched_count: u16,
        changes_required_count: u16,
        self_review_count: u16,
        non_independent_count: u16,
        duplicate_reviewer: bool,
        shared_context: bool,
        conflicting_review: bool,
    ) -> (assessment: Self)
        ensures
            assessment.spec_is_satisfied() == satisfied,
            assessment.spec_approved_count() == approved_count,
            assessment.spec_stale_count() == stale_count,
            assessment.spec_mismatched_count() == mismatched_count,
            assessment.spec_changes_required_count() == changes_required_count,
            assessment.spec_self_review_count() == self_review_count,
            assessment.spec_non_independent_count() == non_independent_count,
            assessment.spec_duplicate_reviewer() == duplicate_reviewer,
            assessment.spec_shared_context() == shared_context,
            assessment.spec_conflicting_review() == conflicting_review,
    {
        Self {
            satisfied,
            approved_count,
            stale_count,
            mismatched_count,
            changes_required_count,
            self_review_count,
            non_independent_count,
            duplicate_reviewer,
            shared_context,
            conflicting_review,
        }
    }

    /// Returns whether the independent-review quorum is clean and complete.
    #[must_use]
    pub const fn is_satisfied(&self) -> (satisfied: bool)
        ensures satisfied == self.spec_is_satisfied()
    {
        self.satisfied
    }

    /// Logical view of whether the independent-review quorum is clean and complete.
    pub closed spec fn spec_is_satisfied(&self) -> bool {
        self.satisfied
    }

    /// Returns the saturated approved-review count.
    #[must_use]
    pub const fn approved_count(&self) -> (count: u16)
        ensures count == self.spec_approved_count()
    { self.approved_count }

    /// Logical view of the approved-review count.
    pub closed spec fn spec_approved_count(&self) -> u16 { self.approved_count }

    /// Returns the saturated stale-review count.
    #[must_use]
    pub const fn stale_count(&self) -> (count: u16)
        ensures count == self.spec_stale_count()
    { self.stale_count }

    /// Logical view of the stale-review count.
    pub closed spec fn spec_stale_count(&self) -> u16 { self.stale_count }

    /// Returns the saturated mismatched-review count.
    #[must_use]
    pub const fn mismatched_count(&self) -> (count: u16)
        ensures count == self.spec_mismatched_count()
    { self.mismatched_count }

    /// Logical view of the mismatched-review count.
    pub closed spec fn spec_mismatched_count(&self) -> u16 { self.mismatched_count }

    /// Returns the saturated changes-required count.
    #[must_use]
    pub const fn changes_required_count(&self) -> (count: u16)
        ensures count == self.spec_changes_required_count()
    { self.changes_required_count }

    /// Logical view of the changes-required count.
    pub closed spec fn spec_changes_required_count(&self) -> u16 {
        self.changes_required_count
    }

    /// Returns the saturated self-review count.
    #[must_use]
    pub const fn self_review_count(&self) -> (count: u16)
        ensures count == self.spec_self_review_count()
    { self.self_review_count }

    /// Logical view of the self-review count.
    pub closed spec fn spec_self_review_count(&self) -> u16 { self.self_review_count }

    /// Returns the saturated non-independent-review count.
    #[must_use]
    pub const fn non_independent_count(&self) -> (count: u16)
        ensures count == self.spec_non_independent_count()
    { self.non_independent_count }

    /// Logical view of the non-independent-review count.
    pub closed spec fn spec_non_independent_count(&self) -> u16 {
        self.non_independent_count
    }

    /// Returns whether current reviews reused a reviewer identity.
    #[must_use]
    pub const fn has_duplicate_reviewer(&self) -> (duplicate: bool)
        ensures duplicate == self.spec_duplicate_reviewer()
    { self.duplicate_reviewer }

    /// Logical view of whether a reviewer identity was duplicated.
    pub closed spec fn spec_duplicate_reviewer(&self) -> bool { self.duplicate_reviewer }

    /// Returns whether current reviews reused a fresh-context digest.
    #[must_use]
    pub const fn has_shared_context(&self) -> (shared: bool)
        ensures shared == self.spec_shared_context()
    { self.shared_context }

    /// Logical view of whether a review context was shared.
    pub closed spec fn spec_shared_context(&self) -> bool { self.shared_context }

    /// Returns whether observations with one review identity disagreed.
    #[must_use]
    pub const fn has_conflicting_review(&self) -> (conflicting: bool)
        ensures conflicting == self.spec_conflicting_review()
    { self.conflicting_review }

    /// Logical view of whether observations for one review conflict.
    pub closed spec fn spec_conflicting_review(&self) -> bool { self.conflicting_review }

    pub(crate) open spec fn spec_diagnostics_clear(&self) -> bool {
        self.spec_approved_count() >= crate::evaluator::MIN_INDEPENDENT_REVIEWERS
            && self.spec_stale_count() == 0
            && self.spec_mismatched_count() == 0
            && self.spec_changes_required_count() == 0
            && self.spec_self_review_count() == 0
            && self.spec_non_independent_count() == 0
            && !self.spec_duplicate_reviewer()
            && !self.spec_shared_context()
            && !self.spec_conflicting_review()
    }
}

} // verus!
