//! Pure executable equality and classification rules shared with Verus proofs.

use peritus_types::{
    ActionId, ActorId, EnvironmentId, Generation, ResourceId, RevisionNumber, WorkspaceId,
};
use vstd::prelude::*;

verus! {

/// Small exact identity projection used by the target-identity refinement.
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

/// Complete scalar projection needed for a one-use mutation permit.
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

/// Returns whether every identifier byte from `index` onward is equal.
pub open spec fn identifier_bytes_equal_from(
    left: [u8; 16],
    right: [u8; 16],
    index: nat,
) -> bool
    decreases 16 - index,
{
    if index >= 16 {
        true
    } else {
        left[index as int] == right[index as int]
            && identifier_bytes_equal_from(left, right, index + 1)
    }
}

const fn identifier_values_equal_from(
    left: [u8; 16],
    right: [u8; 16],
    index: usize,
) -> (result: bool)
    requires index <= 16,
    ensures result == identifier_bytes_equal_from(left, right, index as nat),
    decreases 16 - index,
{
    if index == 16 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        identifier_values_equal_from(left, right, index + 1)
    }
}

/// Returns whether every target and lease identity is nominally exact.
pub open spec fn resource_identity_exact_spec(facts: ResourceFacts) -> bool {
    identifier_bytes_equal_from(facts.target.spec_bytes(), facts.intent.spec_bytes(), 0)
        && identifier_bytes_equal_from(facts.target.spec_bytes(), facts.witness.spec_bytes(), 0)
        && identifier_bytes_equal_from(facts.target.spec_bytes(), facts.capability.spec_bytes(), 0)
        && identifier_bytes_equal_from(
            facts.target.spec_bytes(),
            facts.lease_resource.spec_bytes(),
            0,
        )
        && identifier_bytes_equal_from(
            facts.workspace.spec_bytes(),
            facts.lease_workspace.spec_bytes(),
            0,
        )
        && identifier_bytes_equal_from(
            facts.environment.spec_bytes(),
            facts.lease_environment.spec_bytes(),
            0,
        )
}

/// Returns whether every target and lease identity is nominally exact.
#[must_use]
pub const fn resource_identity_exact(facts: ResourceFacts) -> (result: bool)
    ensures result == resource_identity_exact_spec(facts),
{
    identifier_values_equal_from(facts.target.into_bytes(), facts.intent.into_bytes(), 0)
        && identifier_values_equal_from(facts.target.into_bytes(), facts.witness.into_bytes(), 0)
        && identifier_values_equal_from(
            facts.target.into_bytes(),
            facts.capability.into_bytes(),
            0,
        )
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

/// Returns whether all committed authority dimensions admit one mutation.
pub open spec fn authority_complete_spec(facts: AuthorityFacts) -> bool {
    facts.action_matches
        && facts.actor_matches
        && facts.resource_matches
        && facts.revision_matches
        && facts.lease_matches
        && facts.dispatch_committed
        && facts.time_current
        && facts.generation.spec_value() == facts.expected_generation.spec_value()
        && facts.revision.spec_value() == facts.expected_revision.spec_value()
}

/// Returns whether all committed authority dimensions admit one mutation.
#[must_use]
pub const fn authority_complete(facts: AuthorityFacts) -> (result: bool)
    ensures result == authority_complete_spec(facts),
{
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

/// Complete post-fence observations are the only observations eligible to be clean.
pub open spec fn reconciliation_is_safe_spec(
    correlation_exact: bool,
    inspection_complete: bool,
    transaction_clean: bool,
    git_clean: bool,
) -> bool {
    correlation_exact && inspection_complete && transaction_clean && git_clean
}

/// Complete post-fence observations are the only observations eligible to be clean.
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
) -> (result: bool)
    ensures result == reconciliation_is_safe_spec(
        correlation_exact,
        inspection_complete,
        transaction_clean,
        git_clean,
    ),
{
    correlation_exact && inspection_complete && transaction_clean && git_clean
}

} // verus!

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_types::{ActionId, ActorId, EnvironmentId, ResourceId, WorkspaceId};

    fn workspace(seed: u8) -> WorkspaceId {
        WorkspaceId::new([seed; 16]).unwrap()
    }
    fn resource(seed: u8) -> ResourceId {
        ResourceId::new([seed; 16]).unwrap()
    }
    fn environment(seed: u8) -> EnvironmentId {
        EnvironmentId::new([seed; 16]).unwrap()
    }

    #[test]
    fn resource_identity_rejects_each_independent_mismatch() {
        let exact = ResourceFacts {
            workspace: workspace(1),
            target: resource(2),
            intent: resource(2),
            witness: resource(2),
            capability: resource(2),
            lease_workspace: workspace(1),
            lease_resource: resource(2),
            lease_environment: environment(3),
            environment: environment(3),
        };
        assert!(resource_identity_exact(exact));
        for changed in [
            ResourceFacts { intent: resource(4), ..exact },
            ResourceFacts { witness: resource(4), ..exact },
            ResourceFacts { capability: resource(4), ..exact },
            ResourceFacts { lease_resource: resource(4), ..exact },
            ResourceFacts { lease_workspace: workspace(4), ..exact },
            ResourceFacts { lease_environment: environment(4), ..exact },
        ] {
            assert!(!resource_identity_exact(changed));
        }
    }

    #[test]
    fn authority_gate_rejects_every_missing_fact() {
        let exact = AuthorityFacts {
            action: ActionId::new([1; 16]).unwrap(),
            action_matches: true,
            actor: ActorId::new([2; 16]).unwrap(),
            actor_matches: true,
            resource_matches: true,
            revision_matches: true,
            lease_matches: true,
            dispatch_committed: true,
            time_current: true,
            generation: Generation::first(),
            expected_generation: Generation::first(),
            revision: RevisionNumber::first(),
            expected_revision: RevisionNumber::first(),
        };
        assert!(authority_complete(exact));
        let failures = [
            AuthorityFacts { action_matches: false, ..exact },
            AuthorityFacts { actor_matches: false, ..exact },
            AuthorityFacts { resource_matches: false, ..exact },
            AuthorityFacts { revision_matches: false, ..exact },
            AuthorityFacts { lease_matches: false, ..exact },
            AuthorityFacts { dispatch_committed: false, ..exact },
            AuthorityFacts { time_current: false, ..exact },
            AuthorityFacts { expected_generation: Generation::new(2).unwrap(), ..exact },
            AuthorityFacts { expected_revision: RevisionNumber::new(2).unwrap(), ..exact },
        ];
        assert!(failures.into_iter().all(|facts| !authority_complete(facts)));
    }
}
