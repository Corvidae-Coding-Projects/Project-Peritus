//! Named C2 sandbox refinement obligations.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::verified::{BackendFacts, CompilationFacts};

verus! {

/// `REF-C2-SANDBOX-NO-BROADENING`: a checked plan satisfies every declared domain boundary.
pub proof fn checked_plan_is_no_broader_than_contract(facts: CompilationFacts)
    requires crate::verified::compilation_complete_spec(facts),
    ensures
        (facts.isolation_ordinal == 0 && facts.operation_class_ordinal == 0)
            || (facts.isolation_ordinal == 1 && facts.operation_class_ordinal == 1),
        facts.filesystem_requested == facts.filesystem_admitted,
        facts.process_requested == facts.process_admitted,
        facts.environment_requested == facts.environment_admitted,
        facts.network_requested == facts.network_admitted,
        facts.secrets_requested == facts.secrets_admitted,
        facts.resources_requested == facts.resources_admitted,
        facts.terminal_requested == facts.terminal_admitted,
{
}

/// `REF-C2-SANDBOX-BACKEND-COVERAGE`: admission entails complete enforcement support.
pub proof fn admitted_backend_covers_required_features(facts: BackendFacts)
    requires crate::verified::backend_complete_spec(facts),
    ensures
        facts.required_feature_bits & !facts.supported_feature_bits == 0,
        !(facts.profile_ordinal == 0 && facts.backend_kind_ordinal == 1),
{
}

} // verus!
