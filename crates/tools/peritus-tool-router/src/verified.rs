//! Executable C4 routing predicates with Verus specifications.

use vstd::prelude::*;

verus! {

/// Every independently checked committed authority dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "formal fact projection preserves independently drift-tested authority dimensions"
)]
pub struct ToolAuthorityFacts {
    /// B3 action intent and canonical tool-intent payload are exact.
    pub intent_exact: bool,
    /// B0 action/session lifecycle is exact and dispatched.
    pub lifecycle_exact: bool,
    /// B1 capability use and B0 authorization witness agree.
    pub capability_exact: bool,
    /// Committed held budget reservation is exact and adequate.
    pub budget_exact: bool,
    /// Optional/required lease rule and committed use are exact.
    pub lease_exact: bool,
    /// Receipt contains one exact B0 dispatch event.
    pub dispatch_committed: bool,
    /// Authority epoch, capability validity, and call deadline are current.
    pub time_current: bool,
    /// Complete revision/generation facts agree.
    pub revision_exact: bool,
    /// Registered schema, descriptor, and B1 operation remain exact.
    pub descriptor_exact: bool,
    /// Arguments and prepared-call digests remain exact.
    pub prepared_exact: bool,
}

/// Mathematical conjunction for `REF-C4-B1-AUTHORITY-GATE`.
pub open spec fn tool_authority_complete_spec(facts: ToolAuthorityFacts) -> bool {
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

/// Checks every exact committed fact before permit construction.
#[must_use]
pub const fn tool_authority_complete(facts: ToolAuthorityFacts) -> (result: bool)
    ensures result == tool_authority_complete_spec(facts),
{
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

/// Mathematical canonical exposure intersection.
pub open spec fn tool_exposure_complete_spec(
    registered: bool,
    operation_authenticated: bool,
    role_permits: bool,
    capability_permits: bool,
) -> bool {
    registered && operation_authenticated && role_permits && capability_permits
}

/// Checks descriptor ∩ B1 registry ∩ role ∩ exact capability exposure.
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
) -> (result: bool)
    ensures result == tool_exposure_complete_spec(
        registered,
        operation_authenticated,
        role_permits,
        capability_permits,
    ),
{
    registered && operation_authenticated && role_permits && capability_permits
}

/// Mathematical operation-class refinement for `REF-C4-B1-OPERATION-CLASS`.
pub open spec fn tool_operation_refinement_complete_spec(
    name_exact: bool,
    class_exact: bool,
    mandatory_risk_present: bool,
    side_effect_refines: bool,
) -> bool {
    name_exact && class_exact && mandatory_risk_present && side_effect_refines
}

/// Checks exact descriptor-to-B1 operation refinement.
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
) -> (result: bool)
    ensures result == tool_operation_refinement_complete_spec(
        name_exact,
        class_exact,
        mandatory_risk_present,
        side_effect_refines,
    ),
{
    name_exact && class_exact && mandatory_risk_present && side_effect_refines
}

/// Mathematical invocation lifecycle transition rule.
pub open spec fn tool_lifecycle_transition_valid_spec(
    before: int,
    after: int,
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

/// Checks absent/reserved/active/terminal/indeterminate lifecycle transitions (`0/1/2/3/4`).
#[must_use]
pub const fn tool_lifecycle_transition_valid(
    before: u8,
    after: u8,
    terminal_observed: bool,
) -> (result: bool)
    ensures result == tool_lifecycle_transition_valid_spec(
        before as int,
        after as int,
        terminal_observed,
    ),
{
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

/// Mathematical replay transition: exact idempotent terminal may return, never re-dispatch.
pub open spec fn tool_replay_transition_valid_spec(
    identity_exact: bool,
    terminal: bool,
    idempotent: bool,
    dispatcher_calls: int,
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

/// Checks fail-closed replay behavior.
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
) -> (result: bool)
    ensures result == tool_replay_transition_valid_spec(
        identity_exact,
        terminal,
        idempotent,
        dispatcher_calls as int,
        returned_terminal,
    ),
{
    if !identity_exact {
        dispatcher_calls == 0 && !returned_terminal
    } else if terminal && idempotent {
        dispatcher_calls == 0 && returned_terminal
    } else {
        dispatcher_calls == 0 && !returned_terminal
    }
}

/// Mathematical no-effect-on-rejection predicate.
pub open spec fn tool_rejection_effect_count_valid_spec(
    authority_complete: bool,
    dispatcher_identity_exact: bool,
    effect_count: int,
) -> bool {
    if authority_complete && dispatcher_identity_exact {
        0 <= effect_count <= 1
    } else {
        effect_count == 0
    }
}

/// Checks that malformed/mismatched authority cannot reach a dispatcher effect.
#[must_use]
pub const fn tool_rejection_effect_count_valid(
    authority_complete: bool,
    dispatcher_identity_exact: bool,
    effect_count: u64,
) -> (result: bool)
    ensures result == tool_rejection_effect_count_valid_spec(
        authority_complete,
        dispatcher_identity_exact,
        effect_count as int,
    ),
{
    if authority_complete && dispatcher_identity_exact {
        effect_count <= 1
    } else {
        effect_count == 0
    }
}

} // verus!

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_rejects_each_independent_missing_fact() {
        let exact = ToolAuthorityFacts {
            intent_exact: true,
            lifecycle_exact: true,
            capability_exact: true,
            budget_exact: true,
            lease_exact: true,
            dispatch_committed: true,
            time_current: true,
            revision_exact: true,
            descriptor_exact: true,
            prepared_exact: true,
        };
        assert!(tool_authority_complete(exact));
        let failures = [
            ToolAuthorityFacts { intent_exact: false, ..exact },
            ToolAuthorityFacts { lifecycle_exact: false, ..exact },
            ToolAuthorityFacts { capability_exact: false, ..exact },
            ToolAuthorityFacts { budget_exact: false, ..exact },
            ToolAuthorityFacts { lease_exact: false, ..exact },
            ToolAuthorityFacts { dispatch_committed: false, ..exact },
            ToolAuthorityFacts { time_current: false, ..exact },
            ToolAuthorityFacts { revision_exact: false, ..exact },
            ToolAuthorityFacts { descriptor_exact: false, ..exact },
            ToolAuthorityFacts { prepared_exact: false, ..exact },
        ];
        assert!(failures.into_iter().all(|facts| !tool_authority_complete(facts)));
    }

    #[test]
    fn rejection_and_replay_never_admit_second_effect() {
        assert!(tool_rejection_effect_count_valid(false, true, 0));
        assert!(!tool_rejection_effect_count_valid(false, true, 1));
        assert!(tool_replay_transition_valid(true, true, true, 0, true));
        assert!(!tool_replay_transition_valid(true, true, true, 1, true));
    }

    #[test]
    fn operation_refinement_rejects_each_independent_mismatch() {
        assert!(tool_operation_refinement_complete(true, true, true, true));
        assert!(!tool_operation_refinement_complete(false, true, true, true));
        assert!(!tool_operation_refinement_complete(true, false, true, true));
        assert!(!tool_operation_refinement_complete(true, true, false, true));
        assert!(!tool_operation_refinement_complete(true, true, true, false));
    }
}
