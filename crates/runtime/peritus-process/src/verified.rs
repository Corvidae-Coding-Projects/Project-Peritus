//! Executable scalar rules shared with Verus refinement proofs.

#[doc = " Complete independent comparisons required before constructing an execution permit."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each independently drift-tested authority dimension remains explicit"
)]
pub struct ExecutionAuthorityFacts {
    #[doc = " B3 intent and complete execution payload match."]
    pub intent_exact: bool,
    #[doc = " B0 action and complete parent lineage match."]
    pub lifecycle_exact: bool,
    #[doc = " B1 capability and B0 witness match."]
    pub capability_exact: bool,
    #[doc = " Committed active-effect budget reservation is exact and adequate."]
    pub budget_exact: bool,
    #[doc = " Conditional read-only/writable lease rule is exact."]
    pub lease_exact: bool,
    #[doc = " Exact C0 dispatch frame is present."]
    pub dispatch_committed: bool,
    #[doc = " Authority epoch and half-open validity are current."]
    pub time_current: bool,
    #[doc = " Complete revision and generation facts match."]
    pub revision_exact: bool,
    #[doc = " Sandbox, backend support, preparation, and plan digests match."]
    pub plan_exact: bool,
}
#[doc = " Returns whether every required committed execution-authority comparison succeeded."]
#[must_use]
pub const fn execution_authority_complete(facts: ExecutionAuthorityFacts) -> bool {
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
#[doc = " Returns whether terminal uniqueness and output accounting are complete."]
#[must_use]
pub const fn terminal_accounting_valid(
    terminal_count: u64,
    retained: u64,
    observed: u64,
    dropped: u64,
    tasks_joined: bool,
) -> bool {
    terminal_count == 1 && retained <= observed && dropped == observed - retained && tasks_joined
}
#[doc = " Returns whether exact complete holder inspection establishes quiescence."]
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
) -> bool {
    correlation_exact
        && scan_complete
        && live_processes == 0
        && unresolved_records == 0
        && trees_clean
        && tasks_joined
}
#[doc = " Independent facts required for an exact post-consumption native preparation."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "every native binding is independently drift-tested"
)]
pub struct NativePreparationFacts {
    #[doc = " Complete execution authority was validated and durably consumed."]
    pub authority_consumed: bool,
    #[doc = " Execution and checked sandbox plan digests match."]
    pub plan_exact: bool,
    #[doc = " Admission and selected preparation digests match."]
    pub admission_exact: bool,
    #[doc = " Probed and selected descriptor/support identities match."]
    pub descriptor_exact: bool,
    #[doc = " The implementation platform equals the current native platform."]
    pub platform_exact: bool,
    #[doc = " The helper manifest bytes, digest, and preparation identity match."]
    pub manifest_exact: bool,
}
#[doc = " Checks the complete native preparation binding."]
#[must_use]
pub const fn native_preparation_complete(facts: NativePreparationFacts) -> bool {
    facts.authority_consumed
        && facts.plan_exact
        && facts.admission_exact
        && facts.descriptor_exact
        && facts.platform_exact
        && facts.manifest_exact
}
#[doc = " Checks that every native-owned resource family is released."]
#[must_use]
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "each independently owned cleanup family remains explicit"
)]
pub const fn native_release_complete(
    tree_quiescent: bool,
    terminated: bool,
    backend_released: bool,
    proxy_released: bool,
    secrets_released: bool,
    support_joined: bool,
) -> bool {
    tree_quiescent
        && terminated
        && backend_released
        && proxy_released
        && secrets_released
        && support_joined
}
#[doc = " Checks that unsupported, mismatched, or unauthorized native preparation has no effect."]
#[must_use]
pub const fn native_effect_count_valid(
    supported: bool,
    binding_exact: bool,
    authority_consumed: bool,
    effect_count: u64,
) -> bool {
    if supported && binding_exact && authority_consumed {
        effect_count <= 1
    } else {
        effect_count == 0
    }
}
