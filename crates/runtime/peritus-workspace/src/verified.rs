//! Pure executable equality and classification rules shared with Verus proofs.

use peritus_types::{
    ActionId, ActorId, EnvironmentId, Generation, ResourceId, RevisionNumber, WorkspaceId,
};

#[doc = " Small exact identity projection used by the target-identity refinement."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceFacts {
    pub workspace: WorkspaceId,
    pub target: ResourceId,
    pub intent: ResourceId,
    pub witness: ResourceId,
    pub capability: ResourceId,
    pub lease_workspace: WorkspaceId,
    pub lease_resource: ResourceId,
    pub lease_environment: EnvironmentId,
    pub environment: EnvironmentId,
}
#[doc = " Complete scalar projection needed for a one-use mutation permit."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "formal fact projection preserves each independently verified authority dimension"
)]
pub struct AuthorityFacts {
    pub action: ActionId,
    pub action_matches: bool,
    pub actor: ActorId,
    pub actor_matches: bool,
    pub resource_matches: bool,
    pub revision_matches: bool,
    pub lease_matches: bool,
    pub dispatch_committed: bool,
    pub time_current: bool,
    pub generation: Generation,
    pub expected_generation: Generation,
    pub revision: RevisionNumber,
    pub expected_revision: RevisionNumber,
}
const fn identifier_values_equal_from(left: [u8; 16], right: [u8; 16], index: usize) -> bool {
    if index == 16 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        identifier_values_equal_from(left, right, index + 1)
    }
}
#[doc = " Returns whether every target and lease identity is nominally exact."]
#[must_use]
pub const fn resource_identity_exact(facts: ResourceFacts) -> bool {
    identifier_values_equal_from(facts.target.into_bytes(), facts.intent.into_bytes(), 0)
        && identifier_values_equal_from(facts.target.into_bytes(), facts.witness.into_bytes(), 0)
        && identifier_values_equal_from(facts.target.into_bytes(), facts.capability.into_bytes(), 0)
        && identifier_values_equal_from(
            facts.target.into_bytes(),
            facts.lease_resource.into_bytes(),
            0,
        )
        && identifier_values_equal_from(
            facts.workspace.into_bytes(),
            facts.lease_workspace.into_bytes(),
            0,
        )
        && identifier_values_equal_from(
            facts.environment.into_bytes(),
            facts.lease_environment.into_bytes(),
            0,
        )
}
#[doc = " Returns whether all committed authority dimensions admit one mutation."]
#[must_use]
pub const fn authority_complete(facts: AuthorityFacts) -> bool {
    facts.action_matches
        && facts.actor_matches
        && facts.resource_matches
        && facts.revision_matches
        && facts.lease_matches
        && facts.dispatch_committed
        && facts.time_current
        && facts.generation.get() == facts.expected_generation.get()
        && facts.revision.get() == facts.expected_revision.get()
}
#[doc = " Complete post-fence observations are the only observations eligible to be clean."]
#[must_use]
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "formal refinement keeps each independently drift-tested safety fact explicit"
)]
pub const fn reconciliation_is_safe(
    correlation_exact: bool,
    inspection_complete: bool,
    transaction_clean: bool,
    git_clean: bool,
) -> bool {
    correlation_exact && inspection_complete && transaction_clean && git_clean
}
