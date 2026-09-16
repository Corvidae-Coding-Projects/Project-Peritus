//! Verified finite admission for the production H0 qualification boundary.

use crate::SecurityDecision;
use vstd::prelude::*;

verus! {

/// Number of canonical native probes required by final H0 qualification.
pub const SECURITY_QUALIFICATION_PROBE_COUNT: usize = 42;

/// Terminal state retained for one canonical native probe.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SecurityQualificationOutcome {
    /// Provisioning never produced a runnable subject.
    NotExecuted,
    /// Execution, assertions, resource bounds, or cleanup failed.
    Failed,
    /// Direct execution, assertions, resource bounds, and cleanup all passed.
    Passed,
}

/// Result of the verified finite H0 admission reduction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SecurityQualificationAdmission {
    ready: bool,
}

impl SecurityQualificationAdmission {
    /// Reduces all forty-two canonical outcomes and the verified security decision.
    #[must_use]
    pub const fn evaluate(
        decision: &SecurityDecision,
        outcomes: &[SecurityQualificationOutcome; SECURITY_QUALIFICATION_PROBE_COUNT],
    ) -> (admission: Self)
        ensures admission.spec_is_ready() == (
            decision.spec_is_ready() && spec_security_qualification_outcomes_complete(outcomes)
        )
    {
        let ready = decision.is_ready() && security_qualification_outcomes_complete(outcomes);
        let admission = Self { ready };
        proof {
            reveal(SecurityQualificationAdmission::spec_is_ready);
        }
        admission
    }

    /// Reports whether every native outcome and the verified security decision passed.
    #[must_use]
    pub const fn is_ready(&self) -> (ready: bool)
        ensures ready == self.spec_is_ready()
    {
        self.ready
    }

    /// Specification view of the final H0 admission result.
    pub closed spec fn spec_is_ready(&self) -> bool { self.ready }
}

/// Specification view of the exact forty-two-element native-outcome reduction.
pub open spec fn spec_security_qualification_outcomes_complete(
    outcomes: &[SecurityQualificationOutcome; SECURITY_QUALIFICATION_PROBE_COUNT],
) -> bool {
    spec_security_qualification_outcomes_complete_from(outcomes, 0)
}

pub open spec fn spec_security_qualification_outcomes_complete_from(
    outcomes: &[SecurityQualificationOutcome; SECURITY_QUALIFICATION_PROBE_COUNT],
    index: nat,
) -> bool
    decreases SECURITY_QUALIFICATION_PROBE_COUNT - index,
{
    if index >= SECURITY_QUALIFICATION_PROBE_COUNT {
        true
    } else {
        outcomes[index as int] == SecurityQualificationOutcome::Passed
            && spec_security_qualification_outcomes_complete_from(outcomes, index + 1)
    }
}

const fn security_qualification_outcomes_complete(
    outcomes: &[SecurityQualificationOutcome; SECURITY_QUALIFICATION_PROBE_COUNT],
) -> (complete: bool)
    ensures complete == spec_security_qualification_outcomes_complete(outcomes)
{
    security_qualification_outcomes_complete_from(outcomes, 0)
}

const fn security_qualification_outcomes_complete_from(
    outcomes: &[SecurityQualificationOutcome; SECURITY_QUALIFICATION_PROBE_COUNT],
    index: usize,
) -> (complete: bool)
    requires index <= SECURITY_QUALIFICATION_PROBE_COUNT,
    ensures complete
        == spec_security_qualification_outcomes_complete_from(outcomes, index as nat),
    decreases SECURITY_QUALIFICATION_PROBE_COUNT - index,
{
    if index == SECURITY_QUALIFICATION_PROBE_COUNT {
        true
    } else if !matches!(outcomes[index], SecurityQualificationOutcome::Passed) {
        false
    } else {
        security_qualification_outcomes_complete_from(outcomes, index + 1)
    }
}

} // verus!
