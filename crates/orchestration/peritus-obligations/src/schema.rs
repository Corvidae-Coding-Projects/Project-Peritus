//! Direction-specific request and response schema obligations.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{
    EvidenceBinding, ObligationError, ObligationErrorKind, ObligationLimits, SchemaFieldId,
};

#[path = "schema_coverage.rs"]
mod coverage;
#[path = "schema_model.rs"]
mod model;
#[path = "schema_validation.rs"]
mod validation;

#[doc = " Direction of a public schema contract."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SchemaDirection {
    #[doc = " Fields accepted from the caller."]
    Request,
    #[doc = " Fields emitted to the caller."]
    Response,
}
#[doc = " One exact required schema field."]
#[derive(Debug, Eq, PartialEq)]
pub struct SchemaField {
    id: SchemaFieldId,
    exact_name: Vec<u8>,
}
impl SchemaField {
    #[doc = " Creates a nonempty bounded field name."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects an empty or oversized name."]
    pub fn new(
        id: SchemaFieldId,
        exact_name: Vec<u8>,
        maximum_bytes: usize,
    ) -> Result<Self, ObligationError> {
        if exact_name.is_empty() || exact_name.len() > maximum_bytes {
            Err(ObligationError::numbers(
                ObligationErrorKind::InvalidText,
                maximum_bytes as u64,
                exact_name.len() as u64,
            ))
        } else {
            Ok(Self { id, exact_name })
        }
    }
    #[doc = " Stable direction-specific field identity."]
    #[must_use]
    pub const fn id(&self) -> SchemaFieldId {
        self.id
    }
    #[doc = " Exact public field spelling."]
    #[must_use]
    pub const fn exact_name(&self) -> &[u8] {
        self.exact_name.as_slice()
    }
}
#[doc = " Required fields for one side of a public interface."]
#[derive(Debug, Eq, PartialEq)]
pub struct SchemaRequirement {
    direction: SchemaDirection,
    fields: Vec<SchemaField>,
}
impl SchemaRequirement {
    #[doc = " Creates a nonempty canonical directional field set."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects empty, oversized, duplicate, or unordered field sets."]
    pub fn new(
        direction: SchemaDirection,
        fields: Vec<SchemaField>,
        limits: ObligationLimits,
    ) -> Result<Self, ObligationError> {
        validation::validate_required_fields(fields.as_slice(), limits.max_schema_fields())?;
        Ok(Self { direction, fields })
    }
    #[doc = " Contract direction."]
    #[must_use]
    pub const fn direction(&self) -> SchemaDirection {
        self.direction
    }
    #[doc = " Exact required fields."]
    #[must_use]
    pub const fn fields(&self) -> &[SchemaField] {
        self.fields.as_slice()
    }
}
#[doc = " Candidate observation of one direction-specific schema."]
#[derive(Debug, Eq, PartialEq)]
pub struct SchemaEvidence {
    binding: EvidenceBinding,
    direction: SchemaDirection,
    observed_fields: Vec<SchemaFieldId>,
}
impl SchemaEvidence {
    #[doc = " Creates a canonical observed field set."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects oversized, duplicate, or unordered observed fields."]
    pub fn new(
        binding: EvidenceBinding,
        direction: SchemaDirection,
        observed_fields: Vec<SchemaFieldId>,
        limits: ObligationLimits,
    ) -> Result<Self, ObligationError> {
        validation::validate_observed_fields(
            observed_fields.as_slice(),
            limits.max_schema_fields(),
        )?;
        Ok(Self { binding, direction, observed_fields })
    }
    #[doc = " Complete current-candidate binding."]
    #[must_use]
    pub const fn binding(&self) -> &EvidenceBinding {
        &self.binding
    }
    #[doc = " Observed interface direction."]
    #[must_use]
    pub const fn direction(&self) -> SchemaDirection {
        self.direction
    }
    #[doc = " Canonical observed field identities."]
    #[must_use]
    pub const fn observed_fields(&self) -> &[SchemaFieldId] {
        self.observed_fields.as_slice()
    }
    #[doc = " Whether the evidence covers the exact required direction and fields."]
    #[must_use]
    pub fn covers(&self, requirement: &SchemaRequirement) -> bool {
        coverage::covers(self, requirement)
    }
}
impl SchemaField {
    fn clone_sequence(fields: &[Self]) -> Vec<Self> {
        let mut result = Vec::with_capacity(fields.len());
        let mut index = 0;
        while index < fields.len() {
            result.push(fields[index].clone());
            index += 1;
        }
        result
    }
}
impl Clone for SchemaField {
    fn clone(&self) -> Self {
        let exact_name = self.exact_name.clone();
        Self { id: self.id, exact_name }
    }
}
impl Clone for SchemaRequirement {
    fn clone(&self) -> Self {
        let fields = SchemaField::clone_sequence(self.fields.as_slice());
        Self { direction: self.direction, fields }
    }
}
impl Clone for SchemaEvidence {
    fn clone(&self) -> Self {
        let observed_fields = self.observed_fields.clone();
        Self { binding: self.binding.clone(), direction: self.direction, observed_fields }
    }
}
