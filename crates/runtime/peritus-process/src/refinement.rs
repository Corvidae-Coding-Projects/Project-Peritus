//! Named C2 process-ownership and authority refinement obligations.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::verified::ExecutionAuthorityFacts;

verus! {

/// `OBL-0126`: a private execution permit implies complete committed authority.
pub proof fn permit_implies_complete_execution_authority(facts: ExecutionAuthorityFacts)
    requires crate::verified::execution_authority_complete_spec(facts),
    ensures
        facts.intent_exact,
        facts.lifecycle_exact,
        facts.capability_exact,
        facts.budget_exact,
        facts.lease_exact,
        facts.dispatch_committed,
        facts.time_current,
        facts.revision_exact,
        facts.plan_exact,
{
}

/// `OBL-0127`: holder-quiescence evidence requires exact complete zero-live inspection.
pub proof fn holder_evidence_implies_exact_complete_quiescence(
    correlation_exact: bool,
    scan_complete: bool,
    live_processes: int,
    unresolved_records: int,
    trees_clean: bool,
    tasks_joined: bool,
)
    requires crate::verified::holder_quiescence_exact_spec(
        correlation_exact,
        scan_complete,
        live_processes,
        unresolved_records,
        trees_clean,
        tasks_joined,
    ),
    ensures
        correlation_exact,
        scan_complete,
        live_processes == 0,
        unresolved_records == 0,
        trees_clean,
        tasks_joined,
{
}

/// `INV-013`/`OBL-0129`: terminal publication is unique, bounded, and joined.
pub proof fn terminal_result_implies_unique_owned_completion(
    terminal_count: int,
    retained: int,
    observed: int,
    dropped: int,
    tasks_joined: bool,
)
    requires crate::verified::terminal_accounting_valid_spec(
        terminal_count,
        retained,
        observed,
        dropped,
        tasks_joined,
    ),
    ensures
        terminal_count == 1,
        0 <= retained <= observed,
        dropped == observed - retained,
        tasks_joined,
{
}

} // verus!
