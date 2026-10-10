//! Verified finite admission for the production H4 qualification boundary.

use crate::ReleaseDecision;

#[doc = " Number of deterministic H4 checks retained by final qualification."]
pub const RELEASE_QUALIFICATION_CHECK_COUNT: usize = 8;
#[doc = " Outcome of one deterministic final-qualification check."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReleaseQualificationCheck {
    #[doc = " The exact supplied observations satisfied the check."]
    Satisfied,
    #[doc = " The check rejected or could not establish its obligation."]
    NotSatisfied,
}
#[doc = " Result of the verified finite H4 admission reduction."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReleaseQualificationAdmission {
    ready: bool,
}
impl ReleaseQualificationAdmission {
    #[doc = " Reduces eight fixed qualification outcomes and the verified release decision."]
    #[must_use]
    pub const fn evaluate(
        decision: &ReleaseDecision,
        checks: [ReleaseQualificationCheck; RELEASE_QUALIFICATION_CHECK_COUNT],
    ) -> Self {
        let ready = decision.is_ready() && release_qualification_checks_complete(checks);
        let admission = Self { ready };
        admission
    }
    #[doc = " Reports whether every fixed H4 check and the verified policy decision passed."]
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        self.ready
    }
}
const fn release_qualification_checks_complete(
    checks: [ReleaseQualificationCheck; RELEASE_QUALIFICATION_CHECK_COUNT],
) -> bool {
    release_qualification_checks_complete_from(checks, 0)
}
const fn release_qualification_checks_complete_from(
    checks: [ReleaseQualificationCheck; RELEASE_QUALIFICATION_CHECK_COUNT],
    index: usize,
) -> bool {
    if index == RELEASE_QUALIFICATION_CHECK_COUNT {
        true
    } else if !matches!(checks[index], ReleaseQualificationCheck::Satisfied) {
        false
    } else {
        release_qualification_checks_complete_from(checks, index + 1)
    }
}
