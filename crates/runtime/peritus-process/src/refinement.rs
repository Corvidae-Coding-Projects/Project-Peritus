//! Named C2 process-ownership and authority refinement obligations.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::verified::ExecutionAuthorityFacts;
#[cfg(verus_only)]
use crate::verified::NativePreparationFacts;

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

/// `OBL-0130`: every returned native preparation is exactly bound after durable authority use.
pub proof fn native_preparation_implies_complete_binding(facts: NativePreparationFacts)
    requires crate::verified::native_preparation_complete_spec(facts),
    ensures
        facts.authority_consumed,
        facts.plan_exact,
        facts.admission_exact,
        facts.descriptor_exact,
        facts.platform_exact,
        facts.manifest_exact,
{
}

/// `OBL-0133`: complete native teardown leaves no owned backend resource family live.
pub proof fn complete_native_teardown_releases_every_resource(
    tree_quiescent: bool,
    terminated: bool,
    backend_released: bool,
    proxy_released: bool,
    secrets_released: bool,
    support_joined: bool,
)
    requires crate::verified::native_release_complete_spec(
        tree_quiescent,
        terminated,
        backend_released,
        proxy_released,
        secrets_released,
        support_joined,
    ),
    ensures
        tree_quiescent,
        terminated,
        backend_released,
        proxy_released,
        secrets_released,
        support_joined,
{
}

/// `OBL-0134`: unsupported, mismatched, or unconsumed native preparation has zero effects.
pub proof fn invalid_native_preparation_has_no_effect(
    supported: bool,
    binding_exact: bool,
    authority_consumed: bool,
    effect_count: int,
)
    requires
        crate::verified::native_effect_count_valid_spec(
            supported,
            binding_exact,
            authority_consumed,
            effect_count,
        ),
        !supported || !binding_exact || !authority_consumed,
    ensures effect_count == 0,
{
}

} // verus!
