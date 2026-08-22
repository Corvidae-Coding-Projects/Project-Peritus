//! Shared observation state for one immutable fault plan.

use super::{
    FaultControlError, FaultExpectation, FaultHit, FaultPlan, FaultPoint, FaultVerificationError,
};
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Debug)]
struct FaultState {
    plan: FaultPlan,
    counts: BTreeMap<FaultPoint, u64>,
    triggered: BTreeSet<(FaultPoint, NonZeroU64)>,
}

/// A deterministic snapshot of injector observations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FaultSnapshot {
    counts: BTreeMap<FaultPoint, u64>,
    triggered: Vec<FaultHit>,
}

impl FaultSnapshot {
    /// Returns how often `point` was checked.
    #[must_use]
    pub fn call_count(&self, point: &FaultPoint) -> u64 {
        self.counts.get(point).copied().unwrap_or(0)
    }

    /// Returns activated faults in deterministic point/occurrence order.
    #[must_use]
    pub fn triggered(&self) -> &[FaultHit] {
        &self.triggered
    }
}

/// A shareable injector; clones share call counts and scheduled-hit state.
#[derive(Clone, Debug)]
pub struct FaultInjector {
    state: Arc<Mutex<FaultState>>,
}

impl FaultInjector {
    pub(crate) fn from_plan(plan: FaultPlan) -> Self {
        Self {
            state: Arc::new(Mutex::new(FaultState {
                plan,
                counts: BTreeMap::new(),
                triggered: BTreeSet::new(),
            })),
        }
    }

    /// Records one check and returns the scheduled hit, if any.
    ///
    /// # Errors
    ///
    /// Returns [`FaultControlError`] if the counter overflows or shared state is poisoned.
    pub fn check(&self, point: &FaultPoint) -> Result<Option<FaultHit>, FaultControlError> {
        let mut state = self.lock()?;
        let occurrence = state
            .counts
            .get(point)
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(FaultControlError::OccurrenceOverflow)?;
        state.counts.insert(point.clone(), occurrence);
        let nonzero = NonZeroU64::new(occurrence).ok_or(FaultControlError::OccurrenceOverflow)?;
        let hit = state
            .plan
            .label(point, nonzero)
            .cloned()
            .map(|label| FaultExpectation::new(point.clone(), nonzero, label));
        if hit.is_some() {
            state.triggered.insert((point.clone(), nonzero));
        }
        drop(state);
        Ok(hit)
    }

    /// Captures current counters and activated hits.
    ///
    /// # Errors
    ///
    /// Returns [`FaultControlError::Poisoned`] when shared state is poisoned.
    pub fn snapshot(&self) -> Result<FaultSnapshot, FaultControlError> {
        let state = self.lock()?;
        let triggered = state
            .plan
            .expectations()
            .into_iter()
            .filter(|expectation| {
                state.triggered.contains(&(expectation.point().clone(), expectation.occurrence()))
            })
            .collect();
        Ok(FaultSnapshot { counts: state.counts.clone(), triggered })
    }

    /// Verifies that every scheduled fault was activated.
    ///
    /// # Errors
    ///
    /// Returns missed expectations or a shared-state control failure.
    pub fn verify_all_triggered(&self) -> Result<(), FaultVerificationError> {
        let state = self.lock().map_err(FaultVerificationError::Control)?;
        let expectations: Vec<_> = state
            .plan
            .expectations()
            .into_iter()
            .filter(|expectation| {
                !state.triggered.contains(&(expectation.point().clone(), expectation.occurrence()))
            })
            .collect();
        drop(state);
        if expectations.is_empty() {
            Ok(())
        } else {
            Err(FaultVerificationError::Missed { expectations })
        }
    }

    /// Creates an independent injector with the same plan and zero observations.
    ///
    /// # Errors
    ///
    /// Returns [`FaultControlError::Poisoned`] when shared state is poisoned.
    pub fn fork(&self) -> Result<Self, FaultControlError> {
        Ok(self.lock()?.plan.injector())
    }

    fn lock(&self) -> Result<MutexGuard<'_, FaultState>, FaultControlError> {
        self.state.lock().map_err(|_| FaultControlError::Poisoned)
    }
}
