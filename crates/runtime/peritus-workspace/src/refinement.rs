//! Named C1/B1 refinement obligations.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::verified::{AuthorityFacts, ResourceFacts};

verus! {

/// `REF-C1-B1-RESOURCE-IDENTITY`: accepted target identity is exact in every authority view.
pub proof fn accepted_target_is_exact_authorized_resource(facts: ResourceFacts)
    requires crate::verified::resource_identity_exact_spec(facts),
    ensures
        crate::verified::identifier_bytes_equal_from(
            facts.target.spec_bytes(),
            facts.intent.spec_bytes(),
            0,
        ),
        crate::verified::identifier_bytes_equal_from(
            facts.target.spec_bytes(),
            facts.witness.spec_bytes(),
            0,
        ),
        crate::verified::identifier_bytes_equal_from(
            facts.target.spec_bytes(),
            facts.capability.spec_bytes(),
            0,
        ),
        crate::verified::identifier_bytes_equal_from(
            facts.target.spec_bytes(),
            facts.lease_resource.spec_bytes(),
            0,
        ),
        crate::verified::identifier_bytes_equal_from(
            facts.workspace.spec_bytes(),
            facts.lease_workspace.spec_bytes(),
            0,
        ),
        crate::verified::identifier_bytes_equal_from(
            facts.environment.spec_bytes(),
            facts.lease_environment.spec_bytes(),
            0,
        ),
{
}

/// `REF-C1-B1-AUTHORITY-GATE`: a permit implies every committed authority comparison succeeded.
pub proof fn permit_implies_complete_committed_authority(facts: AuthorityFacts)
    requires crate::verified::authority_complete_spec(facts),
    ensures
        facts.action_matches,
        facts.actor_matches,
        facts.resource_matches,
        facts.revision_matches,
        facts.lease_matches,
        facts.dispatch_committed,
        facts.time_current,
        facts.generation.spec_value() == facts.expected_generation.spec_value(),
        facts.revision.spec_value() == facts.expected_revision.spec_value(),
{
}

/// `REF-C1-B1-RECONCILE-SAFETY`: safe classification never omits a complete exact observation.
pub proof fn safe_assessment_requires_exact_complete_post_fence_observation(
    correlation_exact: bool,
    inspection_complete: bool,
    transaction_clean: bool,
    git_clean: bool,
)
    requires crate::verified::reconciliation_is_safe_spec(
        correlation_exact,
        inspection_complete,
        transaction_clean,
        git_clean,
    ),
    ensures correlation_exact && inspection_complete && transaction_clean && git_clean,
{
}

} // verus!
