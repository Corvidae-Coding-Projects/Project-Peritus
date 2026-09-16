//! Finite executable budget for total dependency propagation.

use vstd::prelude::*;

verus! {

/// Two loop tokens per retained record, plus the pending half-step marker.
pub open spec fn dependency_budget(records: nat, second_half: bool) -> nat {
    (2 as nat) * records + if second_half { 0nat } else { 1nat }
}

proof fn first_half_consumed(records: nat)
    ensures dependency_budget(records, true) + 1 == dependency_budget(records, false),
{
    reveal(dependency_budget);
}

proof fn second_half_consumed(records: nat)
    ensures dependency_budget(records, false) + 1 == dependency_budget(records + 1, true),
{
    reveal(dependency_budget);
}

/// Consumes one loop token without risking arithmetic overflow or underflow.
pub(super) const fn consume(remaining: &mut usize, second_half: &mut bool)
    ensures
        *final(remaining) <= *old(remaining),
        *final(second_half) ==> *final(remaining) > 0,
        *old(remaining) > 0 ==>
            dependency_budget(*final(remaining) as nat, *final(second_half)) + 1
                == dependency_budget(*old(remaining) as nat, *old(second_half)),
{
    if *remaining == 0 {
        *second_half = false;
        return;
    }
    if *second_half {
        *remaining -= 1;
        proof {
            second_half_consumed(*remaining as nat);
        };
        *second_half = false;
    } else {
        proof {
            first_half_consumed(*remaining as nat);
        };
        *second_half = true;
    }
}

} // verus!
