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

mod model;

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
#[derive(Debug, Eq, PartialEq)]
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
    pub const fn class(&self) -> (value: RequirementClass)
        ensures value == self.spec_class(),
    {
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
    pub const fn condition_id(&self) -> (value: Option<ConditionId>)
        ensures value == self.spec_condition_id(),
    {
        match self { Self::Conditional { condition_id } => Some(*condition_id), _ => None }
    }

    /// Alternative group and branch identities.
    #[must_use]
    pub const fn alternative(&self) -> (value: Option<(AlternativeGroupId, AlternativeBranchId)>)
        ensures value == self.spec_alternative(),
    {
        match self {
            Self::Alternative { group_id, branch_id } => Some((*group_id, *branch_id)),
            _ => None,
        }
    }

    /// Whether this entry is informative rather than acceptance-required.
    #[must_use]
    pub const fn is_example(&self) -> (value: bool)
        ensures value == matches!(self, Self::Example),
    { matches!(self, Self::Example) }

    pub(crate) const fn validate(&self) -> (result: Result<(), ObligationError>)
        ensures result.is_ok() == self.spec_valid(),
            match result {
                Err(error) => error.spec_plain(ObligationErrorKind::RequirementShapeMismatch),
                Ok(()) => true,
            },
    {
        match self {
            Self::RequestSchema(requirement)
                if !matches!(requirement.direction(), SchemaDirection::Request) =>
            {
                Err(ObligationError::plain(ObligationErrorKind::RequirementShapeMismatch))
            }
            Self::ResponseSchema(requirement)
                if !matches!(requirement.direction(), SchemaDirection::Response) =>
            {
                Err(ObligationError::plain(ObligationErrorKind::RequirementShapeMismatch))
            }
            _ => Ok(()),
        }
    }

    pub(crate) proof fn same_content_preserves_valid(&self, other: &Self)
        requires self.spec_same_content(other), self.spec_valid(),
        ensures other.spec_valid(),
    {
        match (self, other) {
            (Self::RequestSchema(_), Self::RequestSchema(_)) => {},
            (Self::ResponseSchema(_), Self::ResponseSchema(_)) => {},
            _ => {},
        }
    }

    pub(crate) proof fn same_content_reflexive(&self)
        ensures self.spec_same_content(self),
    {
    }
}

/// One exact public clause and its enforceable obligation shape.
#[derive(Debug, Eq, PartialEq)]
pub struct RequirementEntry {
    id: RequirementId,
    clause: PublicClause,
    specification: ObligationSpec,
    paths: Vec<PathMention>,
}

impl RequirementEntry {
    /// Exact requirement identity.
    pub closed spec fn spec_id(&self) -> RequirementId { self.id }
    /// Exact retained public clause.
    pub closed spec fn spec_clause(&self) -> PublicClause { self.clause }
    /// Exact typed requirement specification.
    pub closed spec fn spec_specification(&self) -> ObligationSpec { self.specification }
    /// Exact ordered path mentions.
    pub closed spec fn spec_paths(&self) -> Seq<PathMention> { self.paths@ }

    /// Intrinsic requirement shape and canonical path invariants.
    pub open spec fn spec_valid(&self) -> bool {
        self.spec_specification().spec_valid()
            && crate::order::ordered(PathMention::keys(self.spec_paths()))
            && crate::order::unique(PathMention::keys(self.spec_paths()))
    }

    #[verifier::type_invariant]
    closed spec fn invariant(&self) -> bool { self.spec_valid() }

    /// Complete semantic content retained by cloning.
    pub open spec fn spec_same_content(&self, other: &Self) -> bool {
        self.spec_id() == other.spec_id()
            && self.spec_clause().spec_same_content(&other.spec_clause())
            && self.spec_specification().spec_same_content(&other.spec_specification())
            && PathMention::sequence_same_content(self.spec_paths(), other.spec_paths())
    }

    /// Complete semantic equality in the same sequence order.
    pub open spec fn sequence_same_content(
        left: Seq<Self>,
        right: Seq<Self>,
    ) -> bool {
        left.len() == right.len() && forall |index: int| 0 <= index < left.len() ==>
            #[trigger] left[index].spec_same_content(&right[index])
    }

    pub(crate) proof fn same_content_preserves_valid(&self, other: &Self)
        requires self.spec_same_content(other), self.spec_valid(),
        ensures other.spec_valid(),
    {
        self.spec_specification().same_content_preserves_valid(&other.spec_specification());
        assert(PathMention::keys(self.spec_paths()) =~= PathMention::keys(other.spec_paths())) by {
            assert forall |index: int| 0 <= index < self.spec_paths().len() implies
                #[trigger] PathMention::keys(self.spec_paths())[index]
                    == #[trigger] PathMention::keys(other.spec_paths())[index] by {
                assert(self.spec_paths()[index].spec_same_content(&other.spec_paths()[index]));
            }
        }
    }

    pub(crate) proof fn entry_same_content_reflexive(&self)
        ensures self.spec_same_content(self),
    {
        self.spec_specification().same_content_reflexive();
    }

    pub(crate) fn new(
        id: RequirementId,
        clause: PublicClause,
        specification: ObligationSpec,
        paths: Vec<PathMention>,
        limits: ObligationLimits,
    ) -> (result: Result<Self, ObligationError>)
        ensures result.is_ok() == (specification.spec_valid()
            && paths@.len() <= limits.spec_max_paths()
            && crate::order::ordered(PathMention::keys(paths@))),
            match result {
                Ok(value) => value.spec_id() == id && value.spec_clause() == clause
                    && value.spec_specification() == specification && value.spec_paths() == paths@
                    && value.spec_valid(),
                Err(error) => if !specification.spec_valid() {
                    error.spec_plain(ObligationErrorKind::RequirementShapeMismatch)
                } else {
                    PathMention::spec_validation_error(paths@, limits.spec_max_paths(), error)
                },
            },
    {
        specification.validate()?;
        crate::path::validate_paths(paths.as_slice(), limits.max_paths_per_requirement())?;
        Ok(Self { id, clause, specification, paths })
    }

    /// Stable requirement identity.
    #[must_use]
    pub const fn id(&self) -> (value: RequirementId)
        ensures value == self.spec_id(),
    { self.id }

    /// Exact authoritative public clause.
    #[must_use]
    pub const fn clause(&self) -> (value: &PublicClause)
        ensures *value == self.spec_clause(),
    { &self.clause }

    /// Typed obligation details.
    #[must_use]
    pub const fn specification(&self) -> (value: &ObligationSpec)
        ensures *value == self.spec_specification(),
    { &self.specification }

    /// Closed public classification.
    #[must_use]
    pub const fn class(&self) -> (value: RequirementClass)
        ensures value == self.spec_specification().spec_class(),
    { self.specification.class() }

    /// Exact path mentions and their distinct roles.
    #[must_use]
    pub const fn paths(&self) -> (value: &[PathMention])
        ensures value@ == self.spec_paths(),
    { self.paths.as_slice() }
}

impl Clone for ObligationSpec {
    fn clone(&self) -> (value: Self)
        ensures self.spec_same_content(&value), self.spec_valid() == value.spec_valid(),
    {
        match self {
            Self::Hard => Self::Hard,
            Self::Conditional { condition_id } => Self::Conditional { condition_id: *condition_id },
            Self::Alternative { group_id, branch_id } => Self::Alternative {
                group_id: *group_id, branch_id: *branch_id,
            },
            Self::Example => Self::Example,
            Self::GeneratedOutput => Self::GeneratedOutput,
            Self::Performance(value) => Self::Performance(*value),
            Self::LifecycleIngress(value) => Self::LifecycleIngress(*value),
            Self::RequestSchema(value) => Self::RequestSchema(value.clone()),
            Self::ResponseSchema(value) => Self::ResponseSchema(value.clone()),
            Self::BrowserSemantics(value) => Self::BrowserSemantics(*value),
            Self::ExternalEffect { effect_identity } => Self::ExternalEffect {
                effect_identity: *effect_identity,
            },
        }
    }
}

impl Clone for RequirementEntry {
    fn clone(&self) -> (value: Self)
        ensures self.spec_same_content(&value),
    {
        proof { use_type_invariant(self); }
        let paths = PathMention::clone_sequence(self.paths.as_slice());
        Self {
            id: self.id, clause: self.clause.clone(),
            specification: self.specification.clone(), paths,
        }
    }
}


impl RequirementEntry {
    pub(crate) fn clone_sequence(entries: &[Self]) -> (result: Vec<Self>)
        ensures Self::sequence_same_content(entries@, result@),
    {
        let mut result = Vec::with_capacity(entries.len());
        let mut index = 0;
        while index < entries.len()
            invariant
                index <= entries.len(),
                result@.len() == index,
                forall |prior: int| 0 <= prior < index ==>
                    #[trigger] entries@[prior].spec_same_content(&result@[prior]),
            decreases entries.len() - index,
        {
            result.push(entries[index].clone());
            index += 1;
        }
        result
    }
}

} // verus!
