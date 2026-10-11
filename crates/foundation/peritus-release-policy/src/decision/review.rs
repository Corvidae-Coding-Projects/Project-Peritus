//! Aggregated independent-review assessment.

#[doc = " Aggregated independent-review state."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "review independence dimensions must remain explicit"
)]
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
    ) -> Self {
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
    #[doc = " Returns whether the independent-review quorum is clean and complete."]
    #[must_use]
    pub const fn is_satisfied(&self) -> bool {
        self.satisfied
    }
    #[doc = " Returns the saturated approved-review count."]
    #[must_use]
    pub const fn approved_count(&self) -> u16 {
        self.approved_count
    }
    #[doc = " Returns the saturated stale-review count."]
    #[must_use]
    pub const fn stale_count(&self) -> u16 {
        self.stale_count
    }
    #[doc = " Returns the saturated mismatched-review count."]
    #[must_use]
    pub const fn mismatched_count(&self) -> u16 {
        self.mismatched_count
    }
    #[doc = " Returns the saturated changes-required count."]
    #[must_use]
    pub const fn changes_required_count(&self) -> u16 {
        self.changes_required_count
    }
    #[doc = " Returns the saturated self-review count."]
    #[must_use]
    pub const fn self_review_count(&self) -> u16 {
        self.self_review_count
    }
    #[doc = " Returns the saturated non-independent-review count."]
    #[must_use]
    pub const fn non_independent_count(&self) -> u16 {
        self.non_independent_count
    }
    #[doc = " Returns whether current reviews reused a reviewer identity."]
    #[must_use]
    pub const fn has_duplicate_reviewer(&self) -> bool {
        self.duplicate_reviewer
    }
    #[doc = " Returns whether current reviews reused a fresh-context digest."]
    #[must_use]
    pub const fn has_shared_context(&self) -> bool {
        self.shared_context
    }
    #[doc = " Returns whether observations with one review identity disagreed."]
    #[must_use]
    pub const fn has_conflicting_review(&self) -> bool {
        self.conflicting_review
    }
}
