//! Owned observations from exactly one provider exercise, including infrastructure failure.

use super::{ProviderConformanceError, ProviderConformanceObservation};
use crate::Observation;

/// One provider exercise outcome and its ordered, redacted supporting evidence.
///
/// Evidence cannot change a failed outcome into success. The suite retains these observations
/// when setup, transport, observation, or cleanup prevents the exercise from completing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderExerciseResult {
    outcome: Result<ProviderConformanceObservation, ProviderConformanceError>,
    observations: Vec<Observation>,
}

impl ProviderExerciseResult {
    /// Creates an owned result for one exercise without retrying or replacing that exercise.
    #[must_use]
    pub const fn new(
        outcome: Result<ProviderConformanceObservation, ProviderConformanceError>,
        observations: Vec<Observation>,
    ) -> Self {
        Self { outcome, observations }
    }

    /// Returns the original outcome and case-supplied observations in their original order.
    pub fn into_parts(
        self,
    ) -> (Result<ProviderConformanceObservation, ProviderConformanceError>, Vec<Observation>) {
        (self.outcome, self.observations)
    }
}
