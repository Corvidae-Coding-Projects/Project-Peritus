//! Executable/refinement predicates for G0 owner invariants.

use peritus_app_protocol::DaemonReadiness;
use vstd::prelude::*;

verus! {

/// Mathematical mutation-admission rule.
pub open spec fn mutation_admitted(readiness: DaemonReadiness) -> bool {
    readiness == DaemonReadiness::ReadyReadWrite
}

/// Mathematical diagnostic-admission rule.
pub open spec fn diagnostic_admitted(readiness: DaemonReadiness) -> bool {
    readiness == DaemonReadiness::ReadyReadWrite
        || readiness == DaemonReadiness::ReadyReadOnly
        || readiness == DaemonReadiness::Draining
}

/// Runtime mutation-admission predicate corresponding to [`mutation_admitted`].
pub fn mutation_admitted_exec(readiness: DaemonReadiness) -> (result: bool)
    ensures result == mutation_admitted(readiness)
{
    readiness.mutation_ready()
}

/// Runtime diagnostic-admission predicate corresponding to [`diagnostic_admitted`].
pub fn diagnostic_admitted_exec(readiness: DaemonReadiness) -> (result: bool)
    ensures result == diagnostic_admitted(readiness)
{
    readiness.diagnostic_ready()
}

} // verus!
