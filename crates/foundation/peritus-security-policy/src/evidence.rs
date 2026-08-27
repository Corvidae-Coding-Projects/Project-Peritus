//! Canonical aggregate supplied to the H0 policy evaluator.

use crate::{
    ArtifactObservation, CriterionObservation, EvidenceCollection, EvidenceError,
    EvidenceErrorKind, IndependentSecurityReview, InventoryObservation, RequirementObservation,
};
use vstd::prelude::*;

verus! {

/// Checked canonical H0 evidence.
#[derive(Debug, Eq, PartialEq)]
pub struct SecurityEvidence {
    requirements: Vec<RequirementObservation>,
    criteria: Vec<CriterionObservation>,
    inventories: Vec<InventoryObservation>,
    artifacts: Vec<ArtifactObservation>,
    review: Option<IndependentSecurityReview>,
}

impl SecurityEvidence {
    /// Specification predicate for exact candidate binding across flat evidence collections.
    pub open spec fn spec_all_current(&self, requested: crate::IntegratedCandidate) -> bool {
        (forall |index: int| 0 <= index < self.spec_requirements().len() ==>
            #[trigger] crate::binding::candidate_fresh(
                self.spec_requirements()[index].spec_candidate(), requested))
        && (forall |index: int| 0 <= index < self.spec_criteria().len() ==>
            #[trigger] crate::binding::candidate_fresh(
                self.spec_criteria()[index].spec_candidate(), requested))
        && (forall |index: int| 0 <= index < self.spec_inventories().len() ==>
            #[trigger] crate::binding::candidate_fresh(
                self.spec_inventories()[index].spec_candidate(), requested))
        && (forall |index: int| 0 <= index < self.spec_artifacts().len() ==>
            #[trigger] crate::binding::candidate_fresh(
                self.spec_artifacts()[index].spec_candidate(), requested))
        && match self.spec_review() {
            Some(review) => crate::binding::candidate_fresh(review.spec_candidate(), requested),
            None => true,
        }
    }

    /// Validates stable order and uniqueness for every evidence collection.
    ///
    /// # Errors
    ///
    /// Returns the first duplicate or ordering violation in collection order.
    pub fn new(
        requirements: Vec<RequirementObservation>,
        criteria: Vec<CriterionObservation>,
        inventories: Vec<InventoryObservation>,
        artifacts: Vec<ArtifactObservation>,
        review: Option<IndependentSecurityReview>,
    ) -> Result<Self, EvidenceError> {
        validate_requirements(requirements.as_slice())?;
        validate_criteria(criteria.as_slice())?;
        validate_inventories(inventories.as_slice())?;
        validate_artifacts(artifacts.as_slice())?;
        Ok(Self { requirements, criteria, inventories, artifacts, review })
    }

    /// Borrows R-SEC observations in literal requirement order.
    #[must_use]
    pub const fn requirements(&self) -> (result: &[RequirementObservation])
        ensures result@ == self.spec_requirements(),
    {
        self.requirements.as_slice()
    }

    /// Borrows numbered criteria in numeric order.
    #[must_use]
    pub const fn criteria(&self) -> (result: &[CriterionObservation])
        ensures result@ == self.spec_criteria(),
    {
        self.criteria.as_slice()
    }

    /// Borrows inventories in canonical role order.
    #[must_use]
    pub const fn inventories(&self) -> (result: &[InventoryObservation])
        ensures result@ == self.spec_inventories(),
    {
        self.inventories.as_slice()
    }

    /// Borrows evidence artifacts in canonical manifest-role order.
    #[must_use]
    pub const fn artifacts(&self) -> (result: &[ArtifactObservation])
        ensures result@ == self.spec_artifacts(),
    {
        self.artifacts.as_slice()
    }

    /// Borrows independently supplied external review evidence, when present.
    #[must_use]
    pub const fn review(&self) -> (result: Option<&IndependentSecurityReview>)
        ensures match (result, self.spec_review()) {
            (Some(actual), Some(expected)) =>
                actual.spec_candidate() == expected.spec_candidate(),
            (None, None) => true,
            _ => false,
        },
    {
        self.review.as_ref()
    }

    /// Specification view of literal requirement observations.
    pub closed spec fn spec_requirements(&self) -> Seq<RequirementObservation> {
        self.requirements@
    }
    /// Specification view of numbered criterion observations.
    pub closed spec fn spec_criteria(&self) -> Seq<CriterionObservation> { self.criteria@ }
    /// Specification view of reviewed inventory observations.
    pub closed spec fn spec_inventories(&self) -> Seq<InventoryObservation> { self.inventories@ }
    /// Specification view of evidence-artifact observations.
    pub closed spec fn spec_artifacts(&self) -> Seq<ArtifactObservation> { self.artifacts@ }
    /// Specification view of independent review evidence.
    pub closed spec fn spec_review(&self) -> Option<IndependentSecurityReview> { self.review }
}

fn validate_requirements(values: &[RequirementObservation]) -> Result<(), EvidenceError> {
    let mut index = 1;
    while index < values.len()
        invariant (values.len() == 0 && index == 1) || 1 <= index <= values.len(),
        decreases values.len() - index,
    {
        let previous = values[index - 1].requirement();
        let current = values[index].requirement();
        if previous == current {
            return Err(EvidenceError::new(
                EvidenceErrorKind::DuplicateObservation,
                EvidenceCollection::Requirements,
                index,
            ));
        }
        if previous > current {
            return Err(EvidenceError::new(
                EvidenceErrorKind::NonCanonicalOrder,
                EvidenceCollection::Requirements,
                index,
            ));
        }
        index += 1;
    }
    Ok(())
}

fn validate_criteria(values: &[CriterionObservation]) -> Result<(), EvidenceError> {
    let mut index = 1;
    while index < values.len()
        invariant (values.len() == 0 && index == 1) || 1 <= index <= values.len(),
        decreases values.len() - index,
    {
        let previous = values[index - 1].criterion();
        let current = values[index].criterion();
        if previous == current {
            return Err(EvidenceError::new(
                EvidenceErrorKind::DuplicateObservation,
                EvidenceCollection::Criteria,
                index,
            ));
        }
        if previous > current {
            return Err(EvidenceError::new(
                EvidenceErrorKind::NonCanonicalOrder,
                EvidenceCollection::Criteria,
                index,
            ));
        }
        index += 1;
    }
    Ok(())
}

fn validate_inventories(values: &[InventoryObservation]) -> Result<(), EvidenceError> {
    let mut index = 1;
    while index < values.len()
        invariant (values.len() == 0 && index == 1) || 1 <= index <= values.len(),
        decreases values.len() - index,
    {
        let previous = values[index - 1].kind();
        let current = values[index].kind();
        if previous == current {
            return Err(EvidenceError::new(
                EvidenceErrorKind::DuplicateObservation,
                EvidenceCollection::Inventories,
                index,
            ));
        }
        if previous > current {
            return Err(EvidenceError::new(
                EvidenceErrorKind::NonCanonicalOrder,
                EvidenceCollection::Inventories,
                index,
            ));
        }
        index += 1;
    }
    Ok(())
}

fn validate_artifacts(values: &[ArtifactObservation]) -> Result<(), EvidenceError> {
    let mut index = 1;
    while index < values.len()
        invariant (values.len() == 0 && index == 1) || 1 <= index <= values.len(),
        decreases values.len() - index,
    {
        let previous = values[index - 1].kind();
        let current = values[index].kind();
        if previous == current {
            return Err(EvidenceError::new(
                EvidenceErrorKind::DuplicateObservation,
                EvidenceCollection::Artifacts,
                index,
            ));
        }
        if previous > current {
            return Err(EvidenceError::new(
                EvidenceErrorKind::NonCanonicalOrder,
                EvidenceCollection::Artifacts,
                index,
            ));
        }
        index += 1;
    }
    Ok(())
}

} // verus!
