//! Typed requirement entries extracted from exact public clauses.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{
    AlternativeBranchId, AlternativeGroupId, BrowserRequirement, ConditionId, LifecycleRequirement,
    ObligationError, ObligationErrorKind, ObligationLimits, PathMention, PerformanceRequirement,
    PublicClause, SchemaDirection, SchemaRequirement,
};
use peritus_spec::RequirementId;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Public requirement classification retained independently of evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RequirementClass {
    Hard,
    Conditional,
    Alternative,
    Example,
    GeneratedOutput,
    Performance,
    LifecycleIngress,
    RequestSchema,
    ResponseSchema,
    BrowserSemantics,
    ExternalEffect,
}

/// Typed semantic details for one public requirement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObligationSpec {
    Hard,
    Conditional { condition_id: ConditionId },
    Alternative { group_id: AlternativeGroupId, branch_id: AlternativeBranchId },
    Example,
    GeneratedOutput,
    Performance(PerformanceRequirement),
    LifecycleIngress(LifecycleRequirement),
    RequestSchema(SchemaRequirement),
    ResponseSchema(SchemaRequirement),
    BrowserSemantics(BrowserRequirement),
    ExternalEffect { effect_identity: Sha256Digest },
}

impl ObligationSpec {
    /// Closed public classification.
    #[must_use]
    pub const fn class(&self) -> RequirementClass {
        match self {
            Self::Hard => RequirementClass::Hard,
            Self::Conditional { .. } => RequirementClass::Conditional,
            Self::Alternative { .. } => RequirementClass::Alternative,
            Self::Example => RequirementClass::Example,
            Self::GeneratedOutput => RequirementClass::GeneratedOutput,
            Self::Performance(_) => RequirementClass::Performance,
            Self::LifecycleIngress(_) => RequirementClass::LifecycleIngress,
            Self::RequestSchema(_) => RequirementClass::RequestSchema,
            Self::ResponseSchema(_) => RequirementClass::ResponseSchema,
            Self::BrowserSemantics(_) => RequirementClass::BrowserSemantics,
            Self::ExternalEffect { .. } => RequirementClass::ExternalEffect,
        }
    }

    /// Public condition identity for a conditional obligation.
    #[must_use]
    pub const fn condition_id(&self) -> Option<ConditionId> {
        match self { Self::Conditional { condition_id } => Some(*condition_id), _ => None }
    }

    /// Alternative group and branch identities.
    #[must_use]
    pub const fn alternative(&self) -> Option<(AlternativeGroupId, AlternativeBranchId)> {
        match self {
            Self::Alternative { group_id, branch_id } => Some((*group_id, *branch_id)),
            _ => None,
        }
    }

    /// Whether this entry is informative rather than acceptance-required.
    #[must_use]
    pub const fn is_example(&self) -> bool { matches!(self, Self::Example) }

    pub(crate) fn validate(&self) -> Result<(), ObligationError> {
        match self {
            Self::RequestSchema(requirement)
                if requirement.direction() != SchemaDirection::Request =>
            {
                Err(ObligationError::plain(ObligationErrorKind::RequirementShapeMismatch))
            }
            Self::ResponseSchema(requirement)
                if requirement.direction() != SchemaDirection::Response =>
            {
                Err(ObligationError::plain(ObligationErrorKind::RequirementShapeMismatch))
            }
            _ => Ok(()),
        }
    }
}

/// One exact public clause and its enforceable obligation shape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequirementEntry {
    id: RequirementId,
    clause: PublicClause,
    specification: ObligationSpec,
    paths: Vec<PathMention>,
}

impl RequirementEntry {
    pub(crate) fn new(
        id: RequirementId,
        clause: PublicClause,
        specification: ObligationSpec,
        paths: Vec<PathMention>,
        limits: ObligationLimits,
    ) -> Result<Self, ObligationError> {
        specification.validate()?;
        crate::path::validate_paths(paths.as_slice(), limits.max_paths_per_requirement())?;
        Ok(Self { id, clause, specification, paths })
    }

    /// Stable requirement identity.
    #[must_use]
    pub const fn id(&self) -> RequirementId { self.id }

    /// Exact authoritative public clause.
    #[must_use]
    pub const fn clause(&self) -> &PublicClause { &self.clause }

    /// Typed obligation details.
    #[must_use]
    pub const fn specification(&self) -> &ObligationSpec { &self.specification }

    /// Closed public classification.
    #[must_use]
    pub const fn class(&self) -> RequirementClass { self.specification.class() }

    /// Exact path mentions and their distinct roles.
    #[must_use]
    pub const fn paths(&self) -> &[PathMention] { self.paths.as_slice() }
}

} // verus!
