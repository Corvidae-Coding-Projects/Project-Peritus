//! Canonical aggregate supplied to the H0 policy evaluator.

use crate::{
    ArtifactObservation, CriterionObservation, EvidenceCollection, EvidenceError,
    EvidenceErrorKind, IndependentSecurityReview, InventoryObservation, RequirementObservation,
};

#[doc = " Checked canonical H0 evidence."]
#[derive(Debug, Eq, PartialEq)]
pub struct SecurityEvidence {
    requirements: Vec<RequirementObservation>,
    criteria: Vec<CriterionObservation>,
    inventories: Vec<InventoryObservation>,
    artifacts: Vec<ArtifactObservation>,
    review: Option<IndependentSecurityReview>,
}
impl SecurityEvidence {
    #[doc = " Validates stable order and uniqueness for every evidence collection."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns the first duplicate or ordering violation in collection order."]
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
    #[doc = " Borrows R-SEC observations in literal requirement order."]
    #[must_use]
    pub const fn requirements(&self) -> &[RequirementObservation] {
        self.requirements.as_slice()
    }
    #[doc = " Borrows numbered criteria in numeric order."]
    #[must_use]
    pub const fn criteria(&self) -> &[CriterionObservation] {
        self.criteria.as_slice()
    }
    #[doc = " Borrows inventories in canonical role order."]
    #[must_use]
    pub const fn inventories(&self) -> &[InventoryObservation] {
        self.inventories.as_slice()
    }
    #[doc = " Borrows evidence artifacts in canonical manifest-role order."]
    #[must_use]
    pub const fn artifacts(&self) -> &[ArtifactObservation] {
        self.artifacts.as_slice()
    }
    #[doc = " Borrows independently supplied external review evidence, when present."]
    #[must_use]
    pub const fn review(&self) -> Option<&IndependentSecurityReview> {
        self.review.as_ref()
    }
}
fn validate_requirements(values: &[RequirementObservation]) -> Result<(), EvidenceError> {
    let mut index = 1;
    while index < values.len() {
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
    while index < values.len() {
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
    while index < values.len() {
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
    while index < values.len() {
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
