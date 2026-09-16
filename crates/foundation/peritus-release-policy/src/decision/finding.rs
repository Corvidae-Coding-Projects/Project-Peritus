//! Aggregated finding and waiver assessment.

use vstd::prelude::*;

verus! {

/// Aggregated finding and waiver state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FindingAssessment {
    satisfied: bool,
    stale_count: u16,
    mismatched_count: u16,
    open_count: u16,
    release_blocking_count: u16,
    ignored_count: u16,
    quarantined_count: u16,
    invalid_waiver_count: u16,
    conflicting_finding: bool,
}

impl FindingAssessment {
    #[allow(clippy::too_many_arguments, reason = "finding and waiver blockers remain explicit")]
    pub(crate) const fn new(
        satisfied: bool,
        stale_count: u16,
        mismatched_count: u16,
        open_count: u16,
        release_blocking_count: u16,
        ignored_count: u16,
        quarantined_count: u16,
        invalid_waiver_count: u16,
        conflicting_finding: bool,
    ) -> (assessment: Self)
        ensures
            assessment.spec_is_satisfied() == satisfied,
            assessment.spec_stale_count() == stale_count,
            assessment.spec_mismatched_count() == mismatched_count,
            assessment.spec_open_count() == open_count,
            assessment.spec_release_blocking_count() == release_blocking_count,
            assessment.spec_ignored_count() == ignored_count,
            assessment.spec_quarantined_count() == quarantined_count,
            assessment.spec_invalid_waiver_count() == invalid_waiver_count,
            assessment.spec_conflicting_finding() == conflicting_finding,
    {
        Self {
            satisfied,
            stale_count,
            mismatched_count,
            open_count,
            release_blocking_count,
            ignored_count,
            quarantined_count,
            invalid_waiver_count,
            conflicting_finding,
        }
    }

    /// Returns whether blockers are absent and every waiver is valid.
    #[must_use]
    pub const fn is_satisfied(&self) -> (satisfied: bool)
        ensures satisfied == self.spec_is_satisfied()
    {
        self.satisfied
    }

    /// Logical view of whether blockers are absent and every waiver is valid.
    pub closed spec fn spec_is_satisfied(&self) -> bool {
        self.satisfied
    }

    /// Returns the saturated stale finding/waiver count.
    #[must_use]
    pub const fn stale_count(&self) -> (count: u16)
        ensures count == self.spec_stale_count()
    { self.stale_count }

    /// Logical view of the stale finding/waiver count.
    pub closed spec fn spec_stale_count(&self) -> u16 { self.stale_count }

    /// Returns the saturated mismatched finding/waiver count.
    #[must_use]
    pub const fn mismatched_count(&self) -> (count: u16)
        ensures count == self.spec_mismatched_count()
    { self.mismatched_count }

    /// Logical view of the mismatched finding/waiver count.
    pub closed spec fn spec_mismatched_count(&self) -> u16 { self.mismatched_count }

    /// Returns the saturated unresolved finding count.
    #[must_use]
    pub const fn open_count(&self) -> (count: u16)
        ensures count == self.spec_open_count()
    { self.open_count }

    /// Logical view of the unresolved finding count.
    pub closed spec fn spec_open_count(&self) -> u16 { self.open_count }

    /// Returns the saturated unresolved release-blocker count.
    #[must_use]
    pub const fn release_blocking_count(&self) -> (count: u16)
        ensures count == self.spec_release_blocking_count()
    { self.release_blocking_count }

    /// Logical view of the unresolved release-blocker count.
    pub closed spec fn spec_release_blocking_count(&self) -> u16 {
        self.release_blocking_count
    }

    /// Returns the saturated ignored-finding count.
    #[must_use]
    pub const fn ignored_count(&self) -> (count: u16)
        ensures count == self.spec_ignored_count()
    { self.ignored_count }

    /// Logical view of the ignored-finding count.
    pub closed spec fn spec_ignored_count(&self) -> u16 { self.ignored_count }

    /// Returns the saturated quarantined-finding count.
    #[must_use]
    pub const fn quarantined_count(&self) -> (count: u16)
        ensures count == self.spec_quarantined_count()
    { self.quarantined_count }

    /// Logical view of the quarantined-finding count.
    pub closed spec fn spec_quarantined_count(&self) -> u16 { self.quarantined_count }

    /// Returns the saturated invalid-waiver count.
    #[must_use]
    pub const fn invalid_waiver_count(&self) -> (count: u16)
        ensures count == self.spec_invalid_waiver_count()
    { self.invalid_waiver_count }

    /// Logical view of the invalid-waiver count.
    pub closed spec fn spec_invalid_waiver_count(&self) -> u16 {
        self.invalid_waiver_count
    }

    /// Returns whether observations with one finding identity disagreed.
    #[must_use]
    pub const fn has_conflicting_finding(&self) -> (conflicting: bool)
        ensures conflicting == self.spec_conflicting_finding()
    { self.conflicting_finding }

    /// Logical view of whether observations for one finding conflict.
    pub closed spec fn spec_conflicting_finding(&self) -> bool {
        self.conflicting_finding
    }

    pub(crate) open spec fn spec_diagnostics_clear(&self) -> bool {
        self.spec_stale_count() == 0
            && self.spec_mismatched_count() == 0
            && self.spec_open_count() == 0
            && self.spec_release_blocking_count() == 0
            && self.spec_ignored_count() == 0
            && self.spec_quarantined_count() == 0
            && self.spec_invalid_waiver_count() == 0
            && !self.spec_conflicting_finding()
    }
}

} // verus!
