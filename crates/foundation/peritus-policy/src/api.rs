//! Stable public facade for the policy authority model.

pub use crate::amendment::{PolicyAmendmentProposal, PolicyRevisionCandidate};
pub use crate::boundary::AuthorityBoundary;
pub use crate::capability::Capability;
pub use crate::capability_failure::CapabilityUseFailure;
pub use crate::capability_issuance::CapabilityIssuanceTransition;
pub use crate::capability_scope::{AuthorizationRequest, CapabilityScope};
pub use crate::capability_transition::CapabilityUseTransition;
pub use crate::capability_use::CapabilityUseRequest;
pub use crate::ceiling::{AuthorityCeiling, CeilingGrant};
pub use crate::decision_enum::PolicyDecision;
pub use crate::decision_reason::AuthorizationDenialReason;
pub use crate::decision_value::{CapabilityIssuancePlan, PolicyDecisionKind};
pub use crate::definition::PolicyDefinition;
pub use crate::denial::AuthorizationDenial;
pub use crate::escalation::EscalationChallenge;
pub use crate::failure::{
    CanonicalCollection, PolicyError, PolicyErrorKind, RecoveryClass, ScopeDimension,
};
pub use crate::independence::{IndependenceRequirement, IndependenceSet};
pub use crate::layer::{PolicyTier, RestrictionLayer};
pub use crate::operation::{OperationDescriptor, OperationRegistry};
pub use crate::risk::{RiskClass, RiskSet};
pub use crate::restriction_rule::RestrictionRule;
pub use crate::role::{ActorRole, OperationClass};
pub use crate::rule::{ApprovalRequirement, AuthorityTier};
pub use crate::scope::{Permission, PermissionSet};
pub use crate::scope_selector::ScopeSelector;
pub use crate::selector::{
    ActorSelector, EnvironmentSelector, PermissionSelector, RoleSelector,
};
pub use crate::time::{AuthorityTimeFailure, AuthorityTimeState};
pub use crate::use_limit::UseLimit;
pub use crate::validity::{AuthorityInstant, ValidityWindow};
