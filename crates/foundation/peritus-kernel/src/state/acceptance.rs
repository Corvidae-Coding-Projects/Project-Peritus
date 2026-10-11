//! Acceptance lifecycle state.

#[doc = " Lifecycle of acceptance evaluation for the current run revision."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AcceptancePhase {
    #[doc = " No evaluation is active for the current candidate."]
    Pending,
    #[doc = " Acceptance evaluation was explicitly begun."]
    Evaluating,
    #[doc = " Evaluation found unmet conditions and the candidate requires work."]
    NeedsChanges,
    #[doc = " The exact current evidence satisfied the exact current contract."]
    Accepted,
    #[doc = " The run terminated without acceptance."]
    Terminated,
}
impl AcceptancePhase {
    #[doc = " Returns whether this phase denotes accepted work."]
    #[must_use]
    pub const fn is_accepted(self) -> bool {
        matches!(self, Self::Accepted)
    }
}
