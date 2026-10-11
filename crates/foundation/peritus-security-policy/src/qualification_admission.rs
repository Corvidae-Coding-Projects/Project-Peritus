//! Verified finite admission for the production H0 qualification boundary.

use crate::SecurityDecision;

#[doc = " Number of canonical native probes required by final H0 qualification."]
pub const SECURITY_QUALIFICATION_PROBE_COUNT: usize = 42;
#[doc = " Terminal state retained for one canonical native probe."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SecurityQualificationOutcome {
    #[doc = " Provisioning never produced a runnable subject."]
    NotExecuted,
    #[doc = " Execution, assertions, resource bounds, or cleanup failed."]
    Failed,
    #[doc = " Direct execution, assertions, resource bounds, and cleanup all passed."]
    Passed,
}
#[doc = " Result of the verified finite H0 admission reduction."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SecurityQualificationAdmission {
    ready: bool,
}
impl SecurityQualificationAdmission {
    #[doc = " Reduces all forty-two canonical outcomes and the verified security decision."]
    #[must_use]
    pub const fn evaluate(
        decision: &SecurityDecision,
        outcomes: &[SecurityQualificationOutcome; SECURITY_QUALIFICATION_PROBE_COUNT],
    ) -> Self {
        let ready = decision.is_ready() && security_qualification_outcomes_complete(outcomes);
        let admission = Self { ready };
        admission
    }
    #[doc = " Reports whether every native outcome and the verified security decision passed."]
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        self.ready
    }
}
const fn security_qualification_outcomes_complete(
    outcomes: &[SecurityQualificationOutcome; SECURITY_QUALIFICATION_PROBE_COUNT],
) -> bool {
    security_qualification_outcomes_complete_from(outcomes, 0)
}
const fn security_qualification_outcomes_complete_from(
    outcomes: &[SecurityQualificationOutcome; SECURITY_QUALIFICATION_PROBE_COUNT],
    index: usize,
) -> bool {
    if index == SECURITY_QUALIFICATION_PROBE_COUNT {
        true
    } else if !matches!(outcomes[index], SecurityQualificationOutcome::Passed) {
        false
    } else {
        security_qualification_outcomes_complete_from(outcomes, index + 1)
    }
}
