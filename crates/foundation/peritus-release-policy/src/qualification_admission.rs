//! Verified finite admission for the production H4 qualification boundary.

use crate::ReleaseDecision;
use vstd::prelude::*;

verus! {

/// Number of deterministic H4 checks retained by final qualification.
pub const RELEASE_QUALIFICATION_CHECK_COUNT: usize = 8;

/// Outcome of one deterministic final-qualification check.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReleaseQualificationCheck {
    /// The exact supplied observations satisfied the check.
    Satisfied,
    /// The check rejected or could not establish its obligation.
    NotSatisfied,
}

/// Result of the verified finite H4 admission reduction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReleaseQualificationAdmission {
    ready: bool,
}

impl ReleaseQualificationAdmission {
    /// Reduces eight fixed qualification outcomes and the verified release decision.
    #[must_use]
    pub const fn evaluate(
        decision: &ReleaseDecision,
        checks: [ReleaseQualificationCheck; RELEASE_QUALIFICATION_CHECK_COUNT],
    ) -> (admission: Self)
        ensures admission.spec_is_ready() == (
            decision.spec_is_ready() && spec_release_qualification_checks_complete(&checks)
        )
    {
        let ready = decision.is_ready() && release_qualification_checks_complete(checks);
        let admission = Self { ready };
        proof {
            reveal(ReleaseQualificationAdmission::spec_is_ready);
        }
        admission
    }

    /// Reports whether every fixed H4 check and the verified policy decision passed.
    #[must_use]
    pub const fn is_ready(&self) -> (ready: bool)
        ensures ready == self.spec_is_ready()
    {
        self.ready
    }

    /// Specification view of the final H4 admission result.
    pub closed spec fn spec_is_ready(&self) -> bool { self.ready }
}

/// Specification view of the exact eight-element H4 check reduction.
pub open spec fn spec_release_qualification_checks_complete(
    checks: &[ReleaseQualificationCheck; RELEASE_QUALIFICATION_CHECK_COUNT],
) -> bool {
    spec_release_qualification_checks_complete_from(checks, 0)
}

pub open spec fn spec_release_qualification_checks_complete_from(
    checks: &[ReleaseQualificationCheck; RELEASE_QUALIFICATION_CHECK_COUNT],
    index: nat,
) -> bool
    decreases RELEASE_QUALIFICATION_CHECK_COUNT - index,
{
    if index >= RELEASE_QUALIFICATION_CHECK_COUNT {
        true
    } else {
        checks[index as int] == ReleaseQualificationCheck::Satisfied
            && spec_release_qualification_checks_complete_from(checks, index + 1)
    }
}

const fn release_qualification_checks_complete(
    checks: [ReleaseQualificationCheck; RELEASE_QUALIFICATION_CHECK_COUNT],
) -> (complete: bool)
    ensures complete == spec_release_qualification_checks_complete(&checks)
{
    release_qualification_checks_complete_from(checks, 0)
}

const fn release_qualification_checks_complete_from(
    checks: [ReleaseQualificationCheck; RELEASE_QUALIFICATION_CHECK_COUNT],
    index: usize,
) -> (complete: bool)
    requires index <= RELEASE_QUALIFICATION_CHECK_COUNT,
    ensures complete == spec_release_qualification_checks_complete_from(&checks, index as nat),
    decreases RELEASE_QUALIFICATION_CHECK_COUNT - index,
{
    if index == RELEASE_QUALIFICATION_CHECK_COUNT {
        true
    } else if !matches!(checks[index], ReleaseQualificationCheck::Satisfied) {
        false
    } else {
        release_qualification_checks_complete_from(checks, index + 1)
    }
}

} // verus!
