//! Ordered, replayable inputs to the deterministic working-state reducer.

#![allow(
    missing_docs,
    reason = "Verus generates undocumented ghost enum projection methods; handwritten API is documented"
)]

use super::{
    ObservationSource, WorkingBinding, WorkingDelta, WorkingEnvironment, WorkingError,
    WorkingState, apply_working_delta, ingest_working_observation, refresh_working_state,
};

#[doc = " A host-authored source/environment event or source-checked model proposal."]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkingEvent {
    #[doc = " Incorporates one exact, already persisted observation."]
    Observation {
        #[doc = " Exact lineage and conversation scope at ingestion."]
        binding: WorkingBinding,
        #[doc = " Verified local artifact locator."]
        source: ObservationSource,
    },
    #[doc = " Refreshes host-observed dependencies without authorizing any effect."]
    Refresh {
        #[doc = " Optimistic state revision."]
        base_revision: u64,
        #[doc = " Current checked environment."]
        environment: WorkingEnvironment,
    },
    #[doc = " Applies an atomic source-backed proposal."]
    Delta(WorkingDelta),
    /// Retains a structurally valid proposal under additional host-observed dependencies.
    /// Unlike model deltas, unknown or changed host inputs retain explicitly stale entries.
    HostDelta {
        /// Exact original proposal, including content, provenance and declared status.
        delta: WorkingDelta,
        /// Constraints captured from its originating request; never model-certified.
        dependencies: super::WorkingValidity,
        /// Conservative source prefix and originating request identity.
        source_binding: super::WorkingHostBinding,
    },
    /// Conservatively invalidates unproven existing conclusions without replacing their content.
    HostInvalidate {
        /// Exact task and role binding.
        binding: WorkingBinding,
        /// Exact optimistic state revision.
        base_revision: u64,
        /// Canonically ordered existing entry identifiers.
        entries: Vec<crate::ContextNodeId>,
    },
    #[doc = " Replaces host-owned active requirement and pending-operation references."]
    Protocol(super::WorkingProtocolUpdate),
}
#[doc = " Reduces exactly one durable input; an error leaves the supplied state unchanged."]
#[doc = ""]
#[doc = " # Errors"]
#[doc = " Propagates binding, source, revision, capacity, and graph validation failures."]
#[allow(clippy::too_many_lines, reason = "each event arm discharges its exact result contract")]
pub fn apply_working_event(
    state: &WorkingState,
    event: &WorkingEvent,
) -> Result<WorkingState, WorkingError> {
    match event {
        WorkingEvent::Observation { binding, source } => {
            match ingest_working_observation(state, *binding, *source) {
                Ok(next) => Ok(next),
                Err(error) => Err(error),
            }
        }
        WorkingEvent::Refresh { base_revision, environment } => {
            let refreshed_environment = environment.clone();
            match refresh_working_state(state, *base_revision, refreshed_environment) {
                Ok(next) => Ok(next),
                Err(error) => Err(error),
            }
        }
        WorkingEvent::Delta(delta) => match apply_working_delta(state, delta) {
            Ok(next) => Ok(next),
            Err(error) => Err(error),
        },
        WorkingEvent::HostDelta { delta, dependencies, source_binding } => {
            super::delta::reduce_delta(state, delta, Some((dependencies, *source_binding)))
        }
        WorkingEvent::HostInvalidate { binding, base_revision, entries } => {
            super::host::invalidate(state, *binding, *base_revision, entries)
        }
        WorkingEvent::Protocol(update) => match super::apply_working_protocol(state, update) {
            Ok(next) => Ok(next),
            Err(error) => Err(error),
        },
    }
}
#[doc = " Reconstructs a bounded committed suffix in its exact recorded order, without effects."]
#[doc = ""]
#[doc = " # Errors"]
#[doc = " Rejects an oversized suffix or the first invalid event; never skips or repairs history."]
pub fn replay_working_events(
    state: &WorkingState,
    events: &[WorkingEvent],
) -> Result<WorkingState, WorkingError> {
    let limits = state.limits();
    let maximum_events = limits.observations();
    if events.len() > maximum_events {
        return Err(WorkingError::Capacity);
    }
    let mut result = state.clone();
    let mut index = 0;
    while index < events.len() {
        let next = match apply_working_event(&result, &events[index]) {
            Ok(next) => next,
            Err(error) => {
                return Err(error);
            }
        };
        result = next;
        index += 1;
    }
    Ok(result)
}
