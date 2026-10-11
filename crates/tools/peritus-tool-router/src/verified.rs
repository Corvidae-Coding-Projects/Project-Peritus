//! Executable C4 routing predicates with Verus specifications.

#[doc = " Every independently checked committed authority dimension."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "formal fact projection preserves independently drift-tested authority dimensions"
)]
pub struct ToolAuthorityFacts {
    #[doc = " B3 action intent and canonical tool-intent payload are exact."]
    pub intent_exact: bool,
    #[doc = " B0 action/session lifecycle is exact and dispatched."]
    pub lifecycle_exact: bool,
    #[doc = " B1 capability use and B0 authorization witness agree."]
    pub capability_exact: bool,
    #[doc = " Committed held budget reservation is exact and adequate."]
    pub budget_exact: bool,
    #[doc = " Optional/required lease rule and committed use are exact."]
    pub lease_exact: bool,
    #[doc = " Receipt contains one exact B0 dispatch event."]
    pub dispatch_committed: bool,
    #[doc = " Authority epoch, capability validity, and call deadline are current."]
    pub time_current: bool,
    #[doc = " Complete revision/generation facts agree."]
    pub revision_exact: bool,
    #[doc = " Registered schema, descriptor, and B1 operation remain exact."]
    pub descriptor_exact: bool,
    #[doc = " Arguments and prepared-call digests remain exact."]
    pub prepared_exact: bool,
}
#[doc = " Checks every exact committed fact before permit construction."]
#[must_use]
pub const fn tool_authority_complete(facts: ToolAuthorityFacts) -> bool {
    facts.intent_exact
        && facts.lifecycle_exact
        && facts.capability_exact
        && facts.budget_exact
        && facts.lease_exact
        && facts.dispatch_committed
        && facts.time_current
        && facts.revision_exact
        && facts.descriptor_exact
        && facts.prepared_exact
}
#[doc = " Checks descriptor ∩ B1 registry ∩ role ∩ exact capability exposure."]
#[must_use]
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "formal exposure conjunction keeps each authority source explicit"
)]
pub const fn tool_exposure_complete(
    registered: bool,
    operation_authenticated: bool,
    role_permits: bool,
    capability_permits: bool,
) -> bool {
    registered && operation_authenticated && role_permits && capability_permits
}
#[doc = " Checks exact descriptor-to-B1 operation refinement."]
#[must_use]
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "formal operation refinement keeps each catalog claim independent"
)]
pub const fn tool_operation_refinement_complete(
    name_exact: bool,
    class_exact: bool,
    mandatory_risk_present: bool,
    side_effect_refines: bool,
) -> bool {
    name_exact && class_exact && mandatory_risk_present && side_effect_refines
}
#[doc = " Checks absent/reserved/active/terminal/indeterminate lifecycle transitions (`0/1/2/3/4`)."]
#[must_use]
pub const fn tool_lifecycle_transition_valid(
    before: u8,
    after: u8,
    terminal_observed: bool,
) -> bool {
    (before == 0 && after == 1)
        || (before == 1 && after == 2)
        || (before == 1 && after == 3 && terminal_observed)
        || (before == 1 && after == 4 && !terminal_observed)
        || (before == 2 && after == 2 && !terminal_observed)
        || (before == 2 && after == 3 && terminal_observed)
        || (before == 2 && after == 4 && !terminal_observed)
        || (before == 3 && after == 3 && terminal_observed)
        || (before == 4 && after == 4 && !terminal_observed)
}
#[doc = " Checks fail-closed replay behavior."]
#[must_use]
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "formal replay projection keeps identity, policy, and effect count separate"
)]
pub const fn tool_replay_transition_valid(
    identity_exact: bool,
    terminal: bool,
    idempotent: bool,
    dispatcher_calls: u64,
    returned_terminal: bool,
) -> bool {
    if !identity_exact {
        dispatcher_calls == 0 && !returned_terminal
    } else if terminal && idempotent {
        dispatcher_calls == 0 && returned_terminal
    } else {
        dispatcher_calls == 0 && !returned_terminal
    }
}
#[doc = " Checks that malformed/mismatched authority cannot reach a dispatcher effect."]
#[must_use]
pub const fn tool_rejection_effect_count_valid(
    authority_complete: bool,
    dispatcher_identity_exact: bool,
    effect_count: u64,
) -> bool {
    if authority_complete && dispatcher_identity_exact {
        effect_count <= 1
    } else {
        effect_count == 0
    }
}
