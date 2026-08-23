//! B1 capability-use fixtures used by kernel integration tests.

use super::{Fixture, bytes, digest, instant, permission, permission_set};
use peritus_policy::{
    ActorRole, ActorSelector, AuthorityBoundary, AuthorityCeiling, AuthorityTimeState,
    AuthorizationRequest, CapabilityScope, CapabilityUseRequest, CapabilityUseTransition,
    CeilingGrant, EnvironmentSelector, OperationClass, OperationDescriptor, OperationRegistry,
    PermissionSelector, PolicyDefinition, RiskClass, RiskSet, RoleSelector, ScopeSelector,
    UseLimit, ValidityWindow,
};
use peritus_types::{ActionId, CapabilityName, CommandId, Sha256Digest};

impl Fixture {
    pub fn capability_use(
        &self,
        action_id: ActionId,
        action_digest: Sha256Digest,
    ) -> CapabilityUseTransition {
        let permission_name = "workspace.mutate";
        let validity = ValidityWindow::new(instant(1, 10), instant(1, 100)).expect("validity");
        let uses = UseLimit::limited(2).expect("use limit");
        let boundary = AuthorityBoundary::new(
            vec![self.actor_id],
            vec![ActorRole::Writer],
            vec![self.environment_id],
            permission_set(self.resource_id, permission_name),
            self.revision,
            validity,
            uses,
        )
        .expect("authority boundary");
        let selector = ScopeSelector::new(
            ActorSelector::any_within_parent(),
            RoleSelector::any_within_parent(),
            EnvironmentSelector::any_within_parent(),
            PermissionSelector::exact(permission_set(self.resource_id, permission_name)),
            self.revision,
        );
        let ceiling = AuthorityCeiling::new(
            boundary,
            vec![CeilingGrant::new(digest(49), selector, validity, uses)],
            Vec::new(),
        )
        .expect("authority ceiling");
        let descriptor = OperationDescriptor::new(
            CapabilityName::new(permission_name.to_owned()).expect("operation name"),
            OperationClass::WorkspaceMutation,
            RiskSet::new(vec![RiskClass::ScopedWrite]).expect("risks"),
        )
        .expect("descriptor");
        let definition = PolicyDefinition::new(
            self.revision.policy_id(),
            ceiling,
            OperationRegistry::new(vec![descriptor]).expect("operation registry"),
            Vec::new(),
        )
        .expect("policy definition");
        let scope = CapabilityScope::new(
            self.actor_id,
            ActorRole::Writer,
            self.environment_id,
            permission_set(self.resource_id, permission_name),
            self.revision,
            validity,
            uses,
        );
        let decision = definition
            .evaluate(
                AuthorizationRequest::new(scope),
                AuthorityTimeState::new(instant(1, 0)),
                instant(1, 10),
            )
            .expect("policy evaluation");
        let plan = decision.into_parts().0.expect("authorized plan");
        let capability = plan
            .issue(CommandId::new(bytes(50)).expect("issuance command"), digest(51))
            .into_capability();
        capability
            .try_use(
                CapabilityUseRequest::new(
                    action_id,
                    action_digest,
                    permission(self.resource_id, permission_name),
                    self.actor_id,
                    ActorRole::Writer,
                    self.environment_id,
                    self.revision,
                    instant(1, 20),
                ),
                digest(52),
            )
            .expect("capability use")
    }
}
