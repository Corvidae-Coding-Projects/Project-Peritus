//! Ordered, replayable inputs to the deterministic working-state reducer.

#![allow(missing_docs, reason = "Verus generates undocumented ghost enum projection methods; handwritten API is documented")]


use super::{ObservationSource, WorkingBinding, WorkingDelta, WorkingEnvironment, WorkingError,
    WorkingState, apply_working_delta, ingest_working_observation, refresh_working_state};
use vstd::prelude::*;

verus! {
/// A host-authored source/environment event or source-checked model proposal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkingEvent {
    /// Incorporates one exact, already persisted observation.
    Observation {
        /// Exact lineage and conversation scope at ingestion.
        binding: WorkingBinding,
        /// Verified local artifact locator.
        source: ObservationSource,
    },
    /// Refreshes host-observed dependencies without authorizing any effect.
    Refresh {
        /// Optimistic state revision.
        base_revision: u64,
        /// Current checked environment.
        environment: WorkingEnvironment,
    },
    /// Applies an atomic source-backed proposal.
    Delta(WorkingDelta),
    /// Replaces host-owned active requirement and pending-operation references.
    Protocol(super::WorkingProtocolUpdate),
}

/// Reduces exactly one durable input; an error leaves the supplied state unchanged.
///
/// # Errors
/// Propagates binding, source, revision, capacity, and graph validation failures.
pub fn apply_working_event(state: &WorkingState, event: &WorkingEvent) -> Result<WorkingState, WorkingError> {
    match event {
        WorkingEvent::Observation { binding, source } => ingest_working_observation(state, *binding, *source),
        WorkingEvent::Refresh { base_revision, environment } => refresh_working_state(state, *base_revision, environment.clone()),
        WorkingEvent::Delta(delta) => apply_working_delta(state, delta),
        WorkingEvent::Protocol(update) => super::apply_working_protocol(state, update),
    }
}

/// Reconstructs a bounded committed suffix in its exact recorded order, without effects.
///
/// # Errors
/// Rejects an oversized suffix or the first invalid event; never skips or repairs history.
pub fn replay_working_events(state: &WorkingState, events: &[WorkingEvent]) -> Result<WorkingState, WorkingError> {
    if events.len() > state.limits.observations() { return Err(WorkingError::Capacity); }
    let mut result = state.clone();
    let mut index = 0;
    while index < events.len()
        invariant index <= events.len(),
        decreases events.len() - index,
    {
        result = apply_working_event(&result, &events[index])?;
        index += 1;
    }
    Ok(result)
}
}
