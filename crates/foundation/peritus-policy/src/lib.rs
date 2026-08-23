//! Verified, effect-free policy, capability, and authority-time decisions for Peritus.
//! Public constructors validate ordinary-Rust inputs; outputs are only logical transitions.

use vstd::prelude::*;

verus! {

mod amendment;
mod amendment_model;
mod api;
mod approval_fold_model;
mod approval_model;
mod boundary;
mod capability;
mod capability_issuance;
mod capability_failure;
mod capability_scope;
mod capability_transition;
mod capability_use;
mod capability_use_model;
mod ceiling;
mod constraint_model;
mod constraint_outcome_model;
mod decision;
mod decision_enum;
mod decision_reason;
mod decision_value;
mod denial;
mod definition;
mod digest_order;
mod failure;
mod identity;
mod independence;
mod evaluation;
mod evaluation_approval;
mod evaluation_approval_result;
mod evaluation_constraint_initial;
mod evaluation_constraints;
mod evaluation_outcome_model;
mod evaluation_outcome_proofs;
mod evaluation_predicates;
pub(crate) mod evaluation_result;
mod escalation;
mod layer;
mod model;
mod monotonicity_model;
mod operation;
mod operation_access;
mod operation_duplication;
mod operation_risk_fold;
mod operation_risks;
mod proofs;
mod restriction_rule;
mod risk;
mod role;
mod rule;
mod scope;
mod scope_validation;
mod scope_selector;
mod selector;
mod time;
mod use_limit;
mod validity;

pub use api::{
    ActorRole, ActorSelector, ApprovalRequirement, AuthorityBoundary, AuthorityCeiling,
    AuthorityInstant, AuthorityTier, AuthorityTimeFailure, AuthorityTimeState,
    AuthorizationDenial, AuthorizationDenialReason, AuthorizationRequest, CanonicalCollection,
    Capability, CapabilityIssuancePlan, CapabilityIssuanceTransition, CapabilityScope,
    CapabilityUseFailure, CapabilityUseRequest, CapabilityUseTransition, CeilingGrant,
    EnvironmentSelector, EscalationChallenge, IndependenceRequirement, IndependenceSet,
    OperationClass, OperationDescriptor, OperationRegistry, Permission, PermissionSelector,
    PermissionSet, PolicyAmendmentProposal, PolicyDecision, PolicyDecisionKind, PolicyDefinition,
    PolicyError, PolicyErrorKind, PolicyRevisionCandidate, PolicyTier, RecoveryClass,
    RestrictionLayer, RestrictionRule, RiskClass, RiskSet, RoleSelector, ScopeDimension,
    ScopeSelector, UseLimit, ValidityWindow,
};

} // verus!
