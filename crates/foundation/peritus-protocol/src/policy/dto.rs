//! Stable data-only policy model used by the wire contract.

use peritus_policy::{
    ActorRole, ActorSelector, ApprovalRequirement, AuthorityBoundary, AuthorityCeiling,
    AuthorityTier, CeilingGrant, EnvironmentSelector, IndependenceRequirement, OperationClass,
    OperationDescriptor, Permission, PolicyTier, RestrictionLayer, RestrictionRule, RiskClass,
    RoleSelector, ScopeSelector, UseLimit, ValidityWindow,
};
use peritus_types::{
    ActorId, CapabilityName, EnvironmentId, ResourceId, RevisionTuple, Sha256Digest,
};

/// Exact resource and capability-name pair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionDto {
    /// Target resource identity.
    pub resource_id: ResourceId,
    /// Registered operation name.
    pub capability_name: CapabilityName,
}

/// Parent-relative selector represented without authority-bearing objects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScopeSelectorDto {
    /// Exact actors, or `None` for parent-bounded wildcard.
    pub actors: Option<Vec<ActorId>>,
    /// Exact roles, or `None` for parent-bounded wildcard.
    pub roles: Option<Vec<ActorRole>>,
    /// Exact environments, or `None` for parent-bounded wildcard.
    pub environments: Option<Vec<EnvironmentId>>,
    /// Exact permission set, or `None` for parent-bounded wildcard.
    pub permissions: Option<Vec<PermissionDto>>,
    /// Exact selector revision.
    pub revision: RevisionTuple,
}

/// Checked approval restriction data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalRequirementDto {
    /// Minimum authenticated authority tier.
    pub minimum_tier: AuthorityTier,
    /// Canonical allowed approver roles.
    pub approver_roles: Vec<ActorRole>,
    /// Canonical independence conjunction.
    pub independence: Vec<IndependenceRequirement>,
    /// Approval validity constraint.
    pub validity: ValidityWindow,
}

/// Closed restriction-rule payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RestrictionRuleKindDto {
    /// Explicit denial.
    Deny,
    /// Approval restriction.
    RequireApproval(ApprovalRequirementDto),
}

/// Canonical lower restriction rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestrictionRuleDto {
    /// Stable rule correlation digest.
    pub digest: Sha256Digest,
    /// Complete checked selector.
    pub selector: ScopeSelectorDto,
    /// Deny or approval restriction.
    pub kind: RestrictionRuleKindDto,
}

/// One authenticated restriction layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestrictionLayerDto {
    /// Authority-tier position.
    pub tier: PolicyTier,
    /// Strict digest-ordered restrictions.
    pub rules: Vec<RestrictionRuleDto>,
}

/// Default-deny ceiling grant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CeilingGrantDto {
    /// Stable grant correlation digest.
    pub digest: Sha256Digest,
    /// Covered selector.
    pub selector: ScopeSelectorDto,
    /// Finite validity constraint.
    pub validity: ValidityWindow,
    /// Finite or unlimited use constraint.
    pub use_limit: UseLimit,
}

/// Complete finite parent authority boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorityBoundaryDto {
    /// Canonical allowed actors.
    pub actors: Vec<ActorId>,
    /// Canonical allowed roles.
    pub roles: Vec<ActorRole>,
    /// Canonical allowed environments.
    pub environments: Vec<EnvironmentId>,
    /// Canonical nonempty permission set.
    pub permissions: Vec<PermissionDto>,
    /// Exact policy revision.
    pub revision: RevisionTuple,
    /// Parent validity bound.
    pub validity: ValidityWindow,
    /// Parent use bound.
    pub use_limit: UseLimit,
}

/// Protected upper authority bound.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorityCeilingDto {
    /// Finite parent boundary.
    pub boundary: AuthorityBoundaryDto,
    /// Canonical grants.
    pub grants: Vec<CeilingGrantDto>,
    /// Canonical immutable deny-only rules.
    pub immutable_denies: Vec<RestrictionRuleDto>,
}

/// Authenticated operation registry entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationDescriptorDto {
    /// Registered capability name.
    pub name: CapabilityName,
    /// Compiled role-separation category.
    pub operation_class: OperationClass,
    /// Canonical nonempty risk set.
    pub risks: Vec<RiskClass>,
}

impl From<&Permission> for PermissionDto {
    fn from(value: &Permission) -> Self {
        Self { resource_id: value.resource_id(), capability_name: value.capability_name().clone() }
    }
}

impl From<&ScopeSelector> for ScopeSelectorDto {
    fn from(value: &ScopeSelector) -> Self {
        Self {
            actors: selector_values(value.actors()),
            roles: role_selector_values(value.roles()),
            environments: environment_selector_values(value.environments()),
            permissions: value
                .permissions()
                .exact_values()
                .map(|set| set.as_slice().iter().map(PermissionDto::from).collect()),
            revision: *value.revision(),
        }
    }
}

impl From<&ApprovalRequirement> for ApprovalRequirementDto {
    fn from(value: &ApprovalRequirement) -> Self {
        Self {
            minimum_tier: value.minimum_tier(),
            approver_roles: value.approver_roles().to_vec(),
            independence: value.independence().as_slice().to_vec(),
            validity: value.validity(),
        }
    }
}

impl From<&RestrictionRule> for RestrictionRuleDto {
    fn from(value: &RestrictionRule) -> Self {
        let kind =
            value.approval_requirement().map_or(RestrictionRuleKindDto::Deny, |requirement| {
                RestrictionRuleKindDto::RequireApproval(requirement.into())
            });
        Self { digest: value.digest(), selector: value.selector().into(), kind }
    }
}

impl From<&RestrictionLayer> for RestrictionLayerDto {
    fn from(value: &RestrictionLayer) -> Self {
        Self {
            tier: value.tier(),
            rules: value.rules().iter().map(RestrictionRuleDto::from).collect(),
        }
    }
}

impl From<&CeilingGrant> for CeilingGrantDto {
    fn from(value: &CeilingGrant) -> Self {
        Self {
            digest: value.digest(),
            selector: value.selector().into(),
            validity: value.validity(),
            use_limit: value.use_limit(),
        }
    }
}

impl From<&AuthorityBoundary> for AuthorityBoundaryDto {
    fn from(value: &AuthorityBoundary) -> Self {
        Self {
            actors: value.actors().to_vec(),
            roles: value.roles().to_vec(),
            environments: value.environments().to_vec(),
            permissions: value.permissions().as_slice().iter().map(PermissionDto::from).collect(),
            revision: *value.revision(),
            validity: value.validity(),
            use_limit: value.use_limit(),
        }
    }
}

impl From<&AuthorityCeiling> for AuthorityCeilingDto {
    fn from(value: &AuthorityCeiling) -> Self {
        Self {
            boundary: value.boundary().into(),
            grants: value.grants().iter().map(CeilingGrantDto::from).collect(),
            immutable_denies: value
                .immutable_denies()
                .iter()
                .map(RestrictionRuleDto::from)
                .collect(),
        }
    }
}

impl From<&OperationDescriptor> for OperationDescriptorDto {
    fn from(value: &OperationDescriptor) -> Self {
        Self {
            name: value.name().clone(),
            operation_class: value.operation_class(),
            risks: value.risks().as_slice().to_vec(),
        }
    }
}

fn selector_values(value: &ActorSelector) -> Option<Vec<ActorId>> {
    value.exact_values().map(<[ActorId]>::to_vec)
}

fn role_selector_values(value: &RoleSelector) -> Option<Vec<ActorRole>> {
    value.exact_values().map(<[ActorRole]>::to_vec)
}

fn environment_selector_values(value: &EnvironmentSelector) -> Option<Vec<EnvironmentId>> {
    value.exact_values().map(<[EnvironmentId]>::to_vec)
}
