//! Explicit bounds for untrusted public clauses and extracted obligation collections.

use crate::{ObligationError, ObligationErrorKind};
use vstd::prelude::*;

verus! {

/// Bounds applied while extracting and qualifying one requirement ledger.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ObligationLimits {
    source_bytes: usize,
    clause_bytes: usize,
    requirements: usize,
    paths_per_requirement: usize,
    schema_fields: usize,
    evidence: usize,
}

impl ObligationLimits {
    /// Creates nonzero bounded limits.
    ///
    /// # Errors
    ///
    /// Rejects any zero bound or a clause bound larger than the complete source bound.
    pub const fn new(
        max_source_bytes: usize,
        max_clause_bytes: usize,
        max_requirements: usize,
        max_paths_per_requirement: usize,
        max_schema_fields: usize,
        max_evidence: usize,
    ) -> Result<Self, ObligationError> {
        if max_source_bytes == 0
            || max_clause_bytes == 0
            || max_requirements == 0
            || max_paths_per_requirement == 0
            || max_schema_fields == 0
            || max_evidence == 0
            || max_clause_bytes > max_source_bytes
        {
            Err(ObligationError::plain(ObligationErrorKind::InvalidLimit))
        } else {
            Ok(Self {
                source_bytes: max_source_bytes,
                clause_bytes: max_clause_bytes,
                requirements: max_requirements,
                paths_per_requirement: max_paths_per_requirement,
                schema_fields: max_schema_fields,
                evidence: max_evidence,
            })
        }
    }

    /// Production defaults sized for a substantial public task request.
    #[must_use]
    pub const fn production() -> Self {
        Self {
            source_bytes: 1_048_576,
            clause_bytes: 65_536,
            requirements: 4_096,
            paths_per_requirement: 256,
            schema_fields: 1_024,
            evidence: 8_192,
        }
    }

    /// Maximum complete public source bytes.
    #[must_use]
    pub const fn max_source_bytes(self) -> usize { self.source_bytes }

    /// Maximum bytes retained for one exact clause.
    #[must_use]
    pub const fn max_clause_bytes(self) -> usize { self.clause_bytes }

    /// Maximum extracted requirements.
    #[must_use]
    pub const fn max_requirements(self) -> usize { self.requirements }

    /// Maximum path mentions on one requirement.
    #[must_use]
    pub const fn max_paths_per_requirement(self) -> usize { self.paths_per_requirement }

    /// Maximum fields in one directional schema obligation.
    #[must_use]
    pub const fn max_schema_fields(self) -> usize { self.schema_fields }

    /// Maximum evidence records supplied for qualification.
    #[must_use]
    pub const fn max_evidence(self) -> usize { self.evidence }
}

} // verus!
