//! Canonical bounded source-event and evidence identifier sets.

use crate::{MemoryError, MemoryErrorKind, MemoryField};
use peritus_types::{EventId, EvidenceId};
use vstd::prelude::*;

verus! {

/// Maximum journal events that may support one memory record.
pub const MAX_SOURCE_EVENTS: usize = 256;
/// Maximum evidence identifiers in either evidence set.
pub const MAX_EVIDENCE_ITEMS: usize = 256;

/// Nonempty canonical set of immutable source events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceEventSet {
    values: Vec<EventId>,
}

impl SourceEventSet {
    /// Validates a bounded, nonempty, strictly increasing event sequence.
    ///
    /// # Errors
    ///
    /// Returns a typed error for empty, oversized, duplicate, or unordered input.
    pub fn new(values: Vec<EventId>) -> Result<Self, MemoryError> {
        validate_nonempty_events(&values)?;
        Ok(Self { values })
    }

    /// Returns source events in canonical order.
    #[must_use]
    pub const fn values(&self) -> &[EventId] { self.values.as_slice() }
}

/// Canonical bounded set of immutable evidence identifiers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceSet {
    values: Vec<EvidenceId>,
}

impl EvidenceSet {
    /// Validates a bounded, strictly increasing evidence sequence. Empty sets are valid.
    ///
    /// # Errors
    ///
    /// Returns a typed error for oversized, duplicate, or unordered input.
    pub fn new(values: Vec<EvidenceId>) -> Result<Self, MemoryError> {
        validate_evidence(&values)?;
        Ok(Self { values })
    }

    /// Returns an empty evidence set.
    #[must_use]
    pub const fn empty() -> Self { Self { values: Vec::new() } }

    /// Returns evidence identifiers in canonical order.
    #[must_use]
    pub const fn values(&self) -> &[EvidenceId] { self.values.as_slice() }

    /// Returns whether this set contains an identifier.
    #[must_use]
    pub fn contains(&self, id: EvidenceId) -> bool {
        let mut index = 0;
        while index < self.values.len()
            invariant index <= self.values.len(),
            decreases self.values.len() - index,
        {
            if self.values[index] == id {
                return true;
            }
            index += 1;
        }
        false
    }

    /// Returns whether the set is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.values.is_empty() }
}

fn validate_nonempty_events(values: &[EventId]) -> Result<(), MemoryError> {
    if values.is_empty() {
        return Err(MemoryError::field(MemoryErrorKind::EmptyValue, MemoryField::SourceEvents));
    }
    if values.len() > MAX_SOURCE_EVENTS {
        return Err(MemoryError::field(MemoryErrorKind::LimitExceeded, MemoryField::SourceEvents));
    }
    let mut index = 1;
    while index < values.len()
        invariant 1 <= index <= values.len(),
        decreases values.len() - index,
    {
        if values[index - 1] == values[index] {
            return Err(MemoryError::field(
                MemoryErrorKind::DuplicateValue,
                MemoryField::SourceEvents,
            ));
        }
        if values[index - 1] > values[index] {
            return Err(MemoryError::field(
                MemoryErrorKind::NonCanonicalOrder,
                MemoryField::SourceEvents,
            ));
        }
        index += 1;
    }
    Ok(())
}

fn validate_evidence(values: &[EvidenceId]) -> Result<(), MemoryError> {
    if values.len() > MAX_EVIDENCE_ITEMS {
        return Err(MemoryError::field(
            MemoryErrorKind::LimitExceeded,
            MemoryField::SupportingEvidence,
        ));
    }
    if values.len() > 1 {
        let mut index = 1;
        while index < values.len()
            invariant 1 <= index <= values.len(),
            decreases values.len() - index,
        {
            if values[index - 1] == values[index] {
                return Err(MemoryError::field(
                    MemoryErrorKind::DuplicateValue,
                    MemoryField::SupportingEvidence,
                ));
            }
            if values[index - 1] > values[index] {
                return Err(MemoryError::field(
                    MemoryErrorKind::NonCanonicalOrder,
                    MemoryField::SupportingEvidence,
                ));
            }
            index += 1;
        }
    }
    Ok(())
}

} // verus!
