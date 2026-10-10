//! Verified, effect-free policy, capability, and authority-time decisions for Peritus.
//! Public constructors validate ordinary-Rust inputs; outputs are only logical transitions.

mod amendment;
mod api;
mod boundary;
mod capability;
mod capability_failure;
mod capability_issuance;
mod capability_scope;
mod capability_transition;
mod capability_use;
mod ceiling;
mod decision;
mod decision_enum;
mod decision_reason;
mod decision_value;
mod definition;
mod denial;
mod digest_order;
mod escalation;
mod evaluation;
mod evaluation_approval;
mod evaluation_approval_result;
mod evaluation_constraint_initial;
mod evaluation_constraints;
mod evaluation_predicates;
pub(crate) mod evaluation_result;
mod failure;
mod identity;
mod independence;
mod layer;
mod operation;
mod operation_access;
mod operation_duplication;
mod operation_risk_fold;
mod operation_risks;
mod restriction_rule;
mod risk;
mod role;
mod rule;
mod scope;
mod scope_selector;
mod scope_validation;
mod selector;
mod time;
mod use_limit;
mod validity;
pub use api::{
    ActorRole, ActorSelector, ApprovalRequirement, AuthorityBoundary, AuthorityCeiling,
    AuthorityInstant, AuthorityTier, AuthorityTimeFailure, AuthorityTimeState, AuthorizationDenial,
    AuthorizationDenialReason, AuthorizationRequest, CanonicalCollection, Capability,
    CapabilityIssuancePlan, CapabilityIssuanceTransition, CapabilityScope, CapabilityUseFailure,
    CapabilityUseRequest, CapabilityUseTransition, CeilingGrant, EnvironmentSelector,
    EscalationChallenge, IndependenceRequirement, IndependenceSet, OperationClass,
    OperationDescriptor, OperationRegistry, Permission, PermissionSelector, PermissionSet,
    PolicyAmendmentProposal, PolicyDecision, PolicyDecisionKind, PolicyDefinition, PolicyError,
    PolicyErrorKind, PolicyRevisionCandidate, PolicyTier, RecoveryClass, RestrictionLayer,
    RestrictionRule, RiskClass, RiskSet, RoleSelector, ScopeDimension, ScopeSelector, UseLimit,
    ValidityWindow,
};
