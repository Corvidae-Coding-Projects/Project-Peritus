//! Direction-specific request and response schema obligations.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{
    EvidenceBinding, ObligationError, ObligationErrorKind, ObligationLimits, SchemaFieldId,
};
use vstd::prelude::*;

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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaField {
    id: SchemaFieldId,
    exact_name: Vec<u8>,
}

impl SchemaField {
    /// Creates a nonempty bounded field name.
    ///
    /// # Errors
    ///
    /// Rejects an empty or oversized name.
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

    /// Stable direction-specific field identity.
    #[must_use]
    pub const fn id(&self) -> SchemaFieldId { self.id }

    /// Exact public field spelling.
    #[must_use]
    pub const fn exact_name(&self) -> &[u8] { self.exact_name.as_slice() }
}

/// Required fields for one side of a public interface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaRequirement {
    direction: SchemaDirection,
    fields: Vec<SchemaField>,
}

impl SchemaRequirement {
    /// Creates a nonempty canonical directional field set.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, duplicate, or unordered field sets.
    pub fn new(
        direction: SchemaDirection,
        fields: Vec<SchemaField>,
        limits: ObligationLimits,
    ) -> Result<Self, ObligationError> {
        if fields.is_empty() || fields.len() > limits.max_schema_fields() {
            return Err(ObligationError::numbers(
                ObligationErrorKind::InvalidSchema,
                limits.max_schema_fields() as u64,
                fields.len() as u64,
            ));
        }
        let mut index = 0;
        while index < fields.len()
            invariant index <= fields.len(),
            decreases fields.len() - index,
        {
            if index > 0 {
                if fields[index - 1].id() == fields[index].id() {
                    return Err(ObligationError::plain(ObligationErrorKind::DuplicateValue));
                }
                if fields[index - 1].id() > fields[index].id() {
                    return Err(ObligationError::plain(ObligationErrorKind::NonCanonicalOrder));
                }
            }
            index += 1;
        }
        Ok(Self { direction, fields })
    }

    /// Contract direction.
    #[must_use]
    pub const fn direction(&self) -> SchemaDirection { self.direction }

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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaEvidence {
    binding: EvidenceBinding,
    direction: SchemaDirection,
    observed_fields: Vec<SchemaFieldId>,
}

impl SchemaEvidence {
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
    ) -> Result<Self, ObligationError> {
        if observed_fields.len() > limits.max_schema_fields() {
            return Err(ObligationError::numbers(
                ObligationErrorKind::InvalidSchema,
                limits.max_schema_fields() as u64,
                observed_fields.len() as u64,
            ));
        }
        let mut index = 0;
        while index < observed_fields.len()
            invariant index <= observed_fields.len(),
            decreases observed_fields.len() - index,
        {
            if index > 0 {
                if observed_fields[index - 1] == observed_fields[index] {
                    return Err(ObligationError::plain(ObligationErrorKind::DuplicateValue));
                }
                if observed_fields[index - 1] > observed_fields[index] {
                    return Err(ObligationError::plain(ObligationErrorKind::NonCanonicalOrder));
                }
            }
            index += 1;
        }
        Ok(Self { binding, direction, observed_fields })
    }

    /// Complete current-candidate binding.
    #[must_use]
    pub const fn binding(&self) -> &EvidenceBinding { &self.binding }

    /// Observed interface direction.
    #[must_use]
    pub const fn direction(&self) -> SchemaDirection { self.direction }

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
    #[allow(clippy::comparison_chain, reason = "explicit comparisons remain Verus-compatible")]
    pub fn covers(&self, requirement: &SchemaRequirement) -> bool {
        if self.direction != requirement.direction() {
            return false;
        }
        let mut required_index = 0;
        let mut observed_index = 0;
        while required_index < requirement.fields().len()
            invariant
                required_index <= requirement.spec_fields().len(),
                observed_index <= self.spec_observed_fields().len(),
            decreases
                (requirement.spec_fields().len() - required_index)
                    + (self.spec_observed_fields().len() - observed_index),
        {
            if observed_index >= self.observed_fields.len() {
                return false;
            }
            let required = requirement.fields()[required_index].id();
            let observed = self.observed_fields[observed_index];
            if observed == required {
                required_index += 1;
                observed_index += 1;
            } else if observed < required {
                observed_index += 1;
            } else {
                return false;
            }
        }
        true
    }
}

} // verus!
