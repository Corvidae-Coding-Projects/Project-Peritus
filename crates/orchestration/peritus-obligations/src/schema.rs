//! Direction-specific request and response schema obligations.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{
    EvidenceBinding, ObligationError, ObligationErrorKind, ObligationLimits, SchemaFieldId,
};
use vstd::prelude::*;

#[path = "schema_coverage.rs"]
mod coverage;
#[path = "schema_model.rs"]
mod model;
#[path = "schema_validation.rs"]
mod validation;

verus! {

/// Direction of a public schema contract.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SchemaDirection {
    /// Fields accepted from the caller.
    Request,
    /// Fields emitted to the caller.
    Response,
}

/// One exact required schema field.
#[derive(Debug, Eq, PartialEq)]
pub struct SchemaField {
    id: SchemaFieldId,
    exact_name: Vec<u8>,
}

impl SchemaField {
    /// Logical view of the complete field identity.
    pub closed spec fn spec_id(&self) -> SchemaFieldId { self.id }

    /// Logical view of every byte in the exact public field spelling.
    pub closed spec fn spec_exact_name(&self) -> Seq<u8> { self.exact_name@ }

    #[verifier::type_invariant]
    closed spec fn invariant(&self) -> bool { self.spec_valid() }

    /// Creates a nonempty bounded field name.
    ///
    /// # Errors
    ///
    /// Rejects an empty or oversized name.
    pub fn new(
        id: SchemaFieldId,
        exact_name: Vec<u8>,
        maximum_bytes: usize,
    ) -> (result: Result<Self, ObligationError>)
        ensures
            result.is_ok() == (0 < exact_name@.len() <= maximum_bytes),
            match result {
                Ok(value) => value.spec_id() == id
                    && value.spec_exact_name() == exact_name@
                    && value.spec_valid(),
                Err(_) => true,
            },
    {
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

    /// Stable direction-specific field identity.
    #[must_use]
    pub const fn id(&self) -> (id: SchemaFieldId)
        ensures id == self.spec_id(),
    { self.id }

    /// Exact public field spelling.
    #[must_use]
    pub const fn exact_name(&self) -> (name: &[u8])
        ensures name@ == self.spec_exact_name(),
    { self.exact_name.as_slice() }
}

/// Required fields for one side of a public interface.
#[derive(Debug, Eq, PartialEq)]
pub struct SchemaRequirement {
    direction: SchemaDirection,
    fields: Vec<SchemaField>,
}

impl SchemaRequirement {
    /// Logical view of the exact request or response direction.
    pub closed spec fn spec_direction(&self) -> SchemaDirection { self.direction }

    #[verifier::type_invariant]
    closed spec fn invariant(&self) -> bool { self.spec_canonical() }

    /// Creates a nonempty canonical directional field set.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, duplicate, or unordered field sets.
    pub fn new(
        direction: SchemaDirection,
        fields: Vec<SchemaField>,
        limits: ObligationLimits,
    ) -> (result: Result<Self, ObligationError>)
        ensures
            result.is_ok() == (0 < fields@.len()
                && fields@.len() <= limits.spec_max_schema_fields()
                && model::required_fields_ordered(fields@)),
            match result {
                Ok(value) => value.spec_direction() == direction
                    && value.spec_fields() == fields@
                    && value.spec_canonical(),
                Err(_) => true,
            },
    {
        validation::validate_required_fields(fields.as_slice(), limits.max_schema_fields())?;
        Ok(Self { direction, fields })
    }

    /// Contract direction.
    #[must_use]
    pub const fn direction(&self) -> (direction: SchemaDirection)
        ensures direction == self.spec_direction(),
    { self.direction }

    /// Exact required fields.
    pub closed spec fn spec_fields(&self) -> Seq<SchemaField> { self.fields@ }

    /// Exact required fields.
    #[must_use]
    pub const fn fields(&self) -> (result: &[SchemaField])
        ensures result@ == self.spec_fields(),
    {
        self.fields.as_slice()
    }
}

/// Candidate observation of one direction-specific schema.
#[derive(Debug, Eq, PartialEq)]
pub struct SchemaEvidence {
    binding: EvidenceBinding,
    direction: SchemaDirection,
    observed_fields: Vec<SchemaFieldId>,
}

impl SchemaEvidence {
    /// Logical view of the exact observed direction.
    pub closed spec fn spec_direction(&self) -> SchemaDirection { self.direction }

    #[verifier::type_invariant]
    closed spec fn invariant(&self) -> bool { self.spec_canonical() }

    /// Exact binding supplied with the schema observation.
    pub closed spec fn spec_binding(&self) -> EvidenceBinding { self.binding }

    /// Creates a canonical observed field set.
    ///
    /// # Errors
    ///
    /// Rejects oversized, duplicate, or unordered observed fields.
    pub fn new(
        binding: EvidenceBinding,
        direction: SchemaDirection,
        observed_fields: Vec<SchemaFieldId>,
        limits: ObligationLimits,
    ) -> (result: Result<Self, ObligationError>)
        ensures
            result.is_ok() == (observed_fields@.len() <= limits.spec_max_schema_fields()
                && model::observed_fields_ordered(observed_fields@)),
            match result {
                Ok(value) => value.spec_binding() == binding
                    && value.spec_direction() == direction
                    && value.spec_observed_fields() == observed_fields@
                    && value.spec_canonical(),
                Err(_) => true,
            },
    {
        validation::validate_observed_fields(
            observed_fields.as_slice(), limits.max_schema_fields())?;
        Ok(Self { binding, direction, observed_fields })
    }

    /// Complete current-candidate binding.
    #[must_use]
    pub const fn binding(&self) -> (value: &EvidenceBinding)
        ensures *value == self.spec_binding(),
    { &self.binding }

    /// Observed interface direction.
    #[must_use]
    pub const fn direction(&self) -> (direction: SchemaDirection)
        ensures direction == self.spec_direction(),
    { self.direction }

    /// Canonical observed field identities.
    pub closed spec fn spec_observed_fields(&self) -> Seq<SchemaFieldId> {
        self.observed_fields@
    }

    /// Canonical observed field identities.
    #[must_use]
    pub const fn observed_fields(&self) -> (result: &[SchemaFieldId])
        ensures result@ == self.spec_observed_fields(),
    {
        self.observed_fields.as_slice()
    }

    /// Whether the evidence covers the exact required direction and fields.
    #[must_use]
    pub fn covers(&self, requirement: &SchemaRequirement) -> (covered: bool)
        ensures covered == self.spec_covers(requirement),
    {
        coverage::covers(self, requirement)
    }
}

impl SchemaField {
    fn clone_sequence(fields: &[Self]) -> (result: Vec<Self>)
        ensures Self::sequence_same_content(fields@, result@),
    {
        let mut result = Vec::with_capacity(fields.len());
        let mut index = 0;
        while index < fields.len()
            invariant
                index <= fields.len(),
                result@.len() == index,
                forall |prior: int| #![auto]
                    0 <= prior < index ==>
                        fields@[prior].spec_same_content(&result@[prior]),
            decreases fields.len() - index,
        {
            result.push(fields[index].clone());
            index += 1;
        }
        result
    }
}

impl Clone for SchemaField {
    fn clone(&self) -> (value: Self)
        ensures self.spec_same_content(&value),
    {
        proof { use_type_invariant(self); }
        let exact_name = self.exact_name.clone();
        proof { assert(exact_name@ =~= self.exact_name@); }
        Self { id: self.id, exact_name }
    }
}

impl Clone for SchemaRequirement {
    fn clone(&self) -> (value: Self)
        ensures self.spec_same_content(&value),
    {
        proof { use_type_invariant(self); }
        let fields = SchemaField::clone_sequence(self.fields.as_slice());
        proof {
            assert(model::required_fields_ordered(fields@)) by {
                assert forall |index: int| 1 <= index < fields@.len() implies
                    crate::order::byte_order(
                        #[trigger] fields@[index - 1].spec_id().spec_digest().spec_bytes()@,
                        #[trigger] fields@[index].spec_id().spec_digest().spec_bytes()@,
                    ) == core::cmp::Ordering::Less by {
                    assert(self.fields@[index - 1].spec_same_content(&fields@[index - 1]));
                    assert(self.fields@[index].spec_same_content(&fields@[index]));
                }
            }
            model::required_ordered_implies_unique(fields@);
        }
        Self { direction: self.direction, fields }
    }
}

impl Clone for SchemaEvidence {
    fn clone(&self) -> (value: Self)
        ensures self.spec_same_content(&value),
    {
        proof { use_type_invariant(self); }
        let observed_fields = self.observed_fields.clone();
        proof {
            assert(observed_fields@ =~= self.observed_fields@);
            assert(model::observed_fields_ordered(observed_fields@));
            model::observed_ordered_implies_unique(observed_fields@);
        }
        Self {
            binding: self.binding.clone(),
            direction: self.direction,
            observed_fields,
        }
    }
}

} // verus!
