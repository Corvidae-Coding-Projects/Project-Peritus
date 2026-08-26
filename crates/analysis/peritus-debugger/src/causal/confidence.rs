//! Explainable bounded integer confidence calculations.

use crate::{DebuggerError, DebuggerErrorKind, DebuggerOperation, DebuggerRecovery};

/// Explainable integer inputs to one confidence value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConfidenceBasis {
    support_count: u32,
    contrary_count: u32,
    ambiguity_count: u32,
    recurrence_count: u32,
    maximum_causal_distance: u32,
}

impl ConfidenceBasis {
    /// Creates bounded confidence counters.
    #[must_use]
    pub const fn new(
        support_count: u32,
        contrary_count: u32,
        ambiguity_count: u32,
        recurrence_count: u32,
        maximum_causal_distance: u32,
    ) -> Self {
        Self {
            support_count,
            contrary_count,
            ambiguity_count,
            recurrence_count,
            maximum_causal_distance,
        }
    }
    /// Returns supporting evidence count.
    #[must_use]
    pub const fn support_count(self) -> u32 {
        self.support_count
    }
    /// Returns contrary evidence count.
    #[must_use]
    pub const fn contrary_count(self) -> u32 {
        self.contrary_count
    }
    /// Returns explicit ambiguity count.
    #[must_use]
    pub const fn ambiguity_count(self) -> u32 {
        self.ambiguity_count
    }
    /// Returns recurrence count.
    #[must_use]
    pub const fn recurrence_count(self) -> u32 {
        self.recurrence_count
    }
    /// Returns maximum causal distance from direct support.
    #[must_use]
    pub const fn maximum_causal_distance(self) -> u32 {
        self.maximum_causal_distance
    }
}

/// Evidence-strength score in integer millionths, never acceptance truth or probability.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConfidenceMillionths {
    value: u32,
    basis: ConfidenceBasis,
}

impl ConfidenceMillionths {
    /// Calculates deterministic bounded evidence strength from explicit counters.
    ///
    /// # Errors
    ///
    /// Rejects a basis without supporting evidence.
    pub fn calculate(basis: ConfidenceBasis) -> Result<Self, DebuggerError> {
        if basis.support_count == 0 {
            return Err(DebuggerError::new(
                DebuggerErrorKind::Report,
                DebuggerOperation::AnalyzeCauses,
                DebuggerRecovery::CorrectInput,
                "cause confidence requires supporting evidence",
            ));
        }
        let support = u64::from(basis.support_count.min(4)) * 150_000;
        let recurrence = u64::from(basis.recurrence_count.min(5)) * 50_000;
        let contrary = u64::from(basis.contrary_count.min(5)) * 100_000;
        let ambiguity = u64::from(basis.ambiguity_count.min(5)) * 100_000;
        let distance = u64::from(basis.maximum_causal_distance.min(10)) * 25_000;
        let positive = 100_000_u64.saturating_add(support).saturating_add(recurrence);
        let value = positive
            .saturating_sub(contrary.saturating_add(ambiguity).saturating_add(distance))
            .clamp(1, 950_000);
        Ok(Self { value: u32::try_from(value).unwrap_or(950_000), basis })
    }

    /// Checks a caller-proposed millionths value and retains its exact basis.
    ///
    /// # Errors
    ///
    /// Rejects zero, values above one million, or certainty with recorded ambiguity.
    pub fn checked(value: u32, basis: ConfidenceBasis) -> Result<Self, DebuggerError> {
        if value == 0 || value > 1_000_000 || (value == 1_000_000 && basis.ambiguity_count > 0) {
            return Err(DebuggerError::new(
                DebuggerErrorKind::Report,
                DebuggerOperation::AnalyzeCauses,
                DebuggerRecovery::CorrectInput,
                "confidence is out of range or claims certainty despite ambiguity",
            ));
        }
        Ok(Self { value, basis })
    }

    /// Returns integer millionths.
    #[must_use]
    pub const fn value(self) -> u32 {
        self.value
    }
    /// Returns the retained calculation basis.
    #[must_use]
    pub const fn basis(self) -> ConfidenceBasis {
        self.basis
    }
}
