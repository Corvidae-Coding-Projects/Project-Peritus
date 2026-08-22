//! Immutable occurrence-addressed fault plans.

use super::{FaultInjector, FaultLabel, FaultPlanError, FaultPoint};
use std::collections::BTreeMap;
use std::num::NonZeroU64;

/// One scheduled fault occurrence.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FaultExpectation {
    point: FaultPoint,
    occurrence: NonZeroU64,
    label: FaultLabel,
}

impl FaultExpectation {
    pub(crate) const fn new(point: FaultPoint, occurrence: NonZeroU64, label: FaultLabel) -> Self {
        Self { point, occurrence, label }
    }

    /// Returns the scheduled point.
    #[must_use]
    pub const fn point(&self) -> &FaultPoint {
        &self.point
    }

    /// Returns the one-based call occurrence.
    #[must_use]
    pub const fn occurrence(&self) -> NonZeroU64 {
        self.occurrence
    }

    /// Returns the caller-interpreted behavior label.
    #[must_use]
    pub const fn label(&self) -> &FaultLabel {
        &self.label
    }
}

/// A scheduled fault that was activated.
pub type FaultHit = FaultExpectation;

/// An immutable set of occurrence-addressed fault expectations.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FaultPlan {
    scheduled: BTreeMap<(FaultPoint, NonZeroU64), FaultLabel>,
}

impl FaultPlan {
    /// Creates an empty plan.
    #[must_use]
    pub const fn new() -> Self {
        Self { scheduled: BTreeMap::new() }
    }

    /// Schedules a label at one exact point occurrence.
    ///
    /// # Errors
    ///
    /// Returns [`FaultPlanError::Duplicate`] instead of overwriting an existing expectation.
    pub fn schedule(
        &mut self,
        point: FaultPoint,
        occurrence: NonZeroU64,
        label: FaultLabel,
    ) -> Result<(), FaultPlanError> {
        let expectation = FaultExpectation::new(point.clone(), occurrence, label.clone());
        if self.scheduled.contains_key(&(point.clone(), occurrence)) {
            return Err(FaultPlanError::Duplicate { expectation });
        }
        self.scheduled.insert((point, occurrence), label);
        Ok(())
    }

    /// Creates a fresh injector with zero observations.
    #[must_use]
    pub fn injector(&self) -> FaultInjector {
        FaultInjector::from_plan(self.clone())
    }

    /// Returns expectations in deterministic point/occurrence order.
    #[must_use]
    pub fn expectations(&self) -> Vec<FaultExpectation> {
        self.scheduled
            .iter()
            .map(|((point, occurrence), label)| {
                FaultExpectation::new(point.clone(), *occurrence, label.clone())
            })
            .collect()
    }

    pub(crate) fn label(&self, point: &FaultPoint, occurrence: NonZeroU64) -> Option<&FaultLabel> {
        self.scheduled.get(&(point.clone(), occurrence))
    }
}
