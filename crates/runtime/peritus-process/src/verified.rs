//! Executable scalar rules shared with Verus refinement proofs.

use vstd::prelude::*;

verus! {

/// Complete independent comparisons required before constructing an execution permit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each independently drift-tested authority dimension remains explicit"
)]
pub struct ExecutionAuthorityFacts {
    /// B3 intent and complete execution payload match.
    pub intent_exact: bool,
    /// B0 action and complete parent lineage match.
    pub lifecycle_exact: bool,
    /// B1 capability and B0 witness match.
    pub capability_exact: bool,
    /// Committed active-effect budget reservation is exact and adequate.
    pub budget_exact: bool,
    /// Conditional read-only/writable lease rule is exact.
    pub lease_exact: bool,
    /// Exact C0 dispatch frame is present.
    pub dispatch_committed: bool,
    /// Authority epoch and half-open validity are current.
    pub time_current: bool,
    /// Complete revision and generation facts match.
    pub revision_exact: bool,
    /// Sandbox, backend support, preparation, and plan digests match.
    pub plan_exact: bool,
}

/// Mathematical authority-gate predicate.
pub open spec fn execution_authority_complete_spec(facts: ExecutionAuthorityFacts) -> bool {
    facts.intent_exact
        && facts.lifecycle_exact
        && facts.capability_exact
        && facts.budget_exact
        && facts.lease_exact
        && facts.dispatch_committed
        && facts.time_current
        && facts.revision_exact
        && facts.plan_exact
}

/// Returns whether every required committed execution-authority comparison succeeded.
#[must_use]
pub const fn execution_authority_complete(
    facts: ExecutionAuthorityFacts,
) -> (result: bool)
    ensures result == execution_authority_complete_spec(facts),
{
    facts.intent_exact
        && facts.lifecycle_exact
        && facts.capability_exact
        && facts.budget_exact
        && facts.lease_exact
        && facts.dispatch_committed
        && facts.time_current
        && facts.revision_exact
        && facts.plan_exact
}

/// Mathematical lifecycle/output terminal acceptance rule.
pub open spec fn terminal_accounting_valid_spec(
    terminal_count: int,
    retained: int,
    observed: int,
    dropped: int,
    tasks_joined: bool,
) -> bool {
    terminal_count == 1
        && 0 <= retained <= observed
        && dropped == observed - retained
        && tasks_joined
}

/// Returns whether terminal uniqueness and output accounting are complete.
#[must_use]
pub const fn terminal_accounting_valid(
    terminal_count: u64,
    retained: u64,
    observed: u64,
    dropped: u64,
    tasks_joined: bool,
) -> (result: bool)
    ensures result == terminal_accounting_valid_spec(
        terminal_count as int,
        retained as int,
        observed as int,
        dropped as int,
        tasks_joined,
    ),
{
    terminal_count == 1
        && retained <= observed
        && dropped == observed - retained
        && tasks_joined
}

/// Mathematical exact holder-quiescence predicate.
pub open spec fn holder_quiescence_exact_spec(
    correlation_exact: bool,
    scan_complete: bool,
    live_processes: int,
    unresolved_records: int,
    trees_clean: bool,
    tasks_joined: bool,
) -> bool {
    correlation_exact
        && scan_complete
        && live_processes == 0
        && unresolved_records == 0
        && trees_clean
        && tasks_joined
}

/// Returns whether exact complete holder inspection establishes quiescence.
#[must_use]
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "the refinement predicate keeps independently proved facts explicit"
)]
pub const fn holder_quiescence_exact(
    correlation_exact: bool,
    scan_complete: bool,
    live_processes: u64,
    unresolved_records: u64,
    trees_clean: bool,
    tasks_joined: bool,
) -> (result: bool)
    ensures result == holder_quiescence_exact_spec(
        correlation_exact,
        scan_complete,
        live_processes as int,
        unresolved_records as int,
        trees_clean,
        tasks_joined,
    ),
{
    correlation_exact
        && scan_complete
        && live_processes == 0
        && unresolved_records == 0
        && trees_clean
        && tasks_joined
}

} // verus!
