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

/// Complete successful one-event frame used by ordered replay.
pub open spec fn event_success_frame(
    state: &WorkingState,
    event: &WorkingEvent,
    next: &WorkingState,
) -> bool {
    match event {
        WorkingEvent::Observation { source, .. } => {
            &&& next.spec_environment().spec_binding()
                == state.spec_environment().spec_binding()
            &&& next.spec_environment().spec_candidate()
                == state.spec_environment().spec_candidate()
            &&& next.spec_environment().spec_files()
                == state.spec_environment().spec_files()
            &&& next.spec_revision() >= state.spec_revision()
            &&& next.spec_revision() as int <= state.spec_revision() as int + 1
            &&& next.spec_observations().len() >= state.spec_observations().len()
            &&& next.spec_observations().len() <= state.spec_observations().len() + 1
            &&& (source.spec_id().spec_value() <= state.spec_observations().len()
                ==> next.spec_revision() == state.spec_revision()
                    && next.spec_observations().len() == state.spec_observations().len())
            &&& (source.spec_id().spec_value() > state.spec_observations().len()
                ==> next.spec_revision() as int == state.spec_revision() as int + 1
                    && next.spec_observations() == state.spec_observations().push(*source))
            &&& super::WorkingEntry::sequence_clone_equivalent(
                state.spec_entries(), next.spec_entries(),
            )
            &&& next.spec_limits() == state.spec_limits()
            &&& next.spec_protocol().spec_requirements()
                == state.spec_protocol().spec_requirements()
            &&& next.spec_protocol().spec_pending()
                == state.spec_protocol().spec_pending()
        }
        WorkingEvent::Refresh { environment, .. } => {
            &&& next.spec_environment().spec_binding() == environment.spec_binding()
            &&& next.spec_environment().spec_candidate() == environment.spec_candidate()
            &&& next.spec_environment().spec_files() == environment.spec_files()
            &&& next.spec_revision() >= state.spec_revision()
            &&& next.spec_revision() as int <= state.spec_revision() as int + 1
            &&& next.spec_observations() == state.spec_observations()
            &&& super::WorkingEntry::sequence_payload_equivalent(
                state.spec_entries(), next.spec_entries(),
            )
            &&& next.spec_limits() == state.spec_limits()
            &&& next.spec_protocol().spec_requirements()
                == state.spec_protocol().spec_requirements()
            &&& next.spec_protocol().spec_pending()
                == state.spec_protocol().spec_pending()
        }
        WorkingEvent::Delta(_) => {
            &&& next.spec_environment().spec_binding()
                == state.spec_environment().spec_binding()
            &&& next.spec_environment().spec_candidate()
                == state.spec_environment().spec_candidate()
            &&& next.spec_environment().spec_files()
                == state.spec_environment().spec_files()
            &&& next.spec_revision() as int == state.spec_revision() as int + 1
            &&& next.spec_observations() == state.spec_observations()
            &&& next.spec_limits() == state.spec_limits()
            &&& next.spec_protocol().spec_requirements()
                == state.spec_protocol().spec_requirements()
            &&& next.spec_protocol().spec_pending()
                == state.spec_protocol().spec_pending()
        }
        WorkingEvent::Protocol(update) => {
            &&& next.spec_environment().spec_binding()
                == state.spec_environment().spec_binding()
            &&& next.spec_environment().spec_candidate()
                == state.spec_environment().spec_candidate()
            &&& next.spec_environment().spec_files()
                == state.spec_environment().spec_files()
            &&& next.spec_revision() as int == state.spec_revision() as int + 1
            &&& next.spec_observations() == state.spec_observations()
            &&& super::WorkingEntry::sequence_clone_equivalent(
                state.spec_entries(), next.spec_entries(),
            )
            &&& next.spec_limits() == state.spec_limits()
            &&& next.spec_protocol().spec_requirements()
                == update.spec_protocol().spec_requirements()
            &&& next.spec_protocol().spec_pending()
                == update.spec_protocol().spec_pending()
        }
    }
}

/// Exact ordered successful prefix: every successor is the certified result of one event.
pub open spec fn replay_success_prefix(
    initial: &WorkingState,
    events: Seq<WorkingEvent>,
    count: int,
    current: &WorkingState,
) -> bool
    decreases count,
{
    if count <= 0 {
        initial.spec_same(current)
    } else {
        count <= events.len()
            && exists |previous: WorkingState|
                replay_success_prefix(initial, events, count - 1, &previous)
                    && #[trigger] event_success_frame(
                        &previous,
                        &events[count as int - 1],
                        current,
                    )
    }
}

/// Reduces exactly one durable input; an error leaves the supplied state unchanged.
///
/// # Errors
/// Propagates binding, source, revision, capacity, and graph validation failures.
pub fn apply_working_event(
    state: &WorkingState,
    event: &WorkingEvent,
) -> (result: Result<WorkingState, WorkingError>)
    ensures match result {
        Ok(next) => event_success_frame(state, event, &next),
        Err(_) => true,
    },
{
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
pub fn replay_working_events(
    state: &WorkingState,
    events: &[WorkingEvent],
) -> (result: Result<WorkingState, WorkingError>)
    ensures match result {
        Ok(next) => replay_success_prefix(state, events@, events@.len() as int, &next),
        Err(_) => true,
    },
{
    if events.len() > state.limits.observations() { return Err(WorkingError::Capacity); }
    let mut result = state.clone();
    proof { assert(replay_success_prefix(state, events@, 0, &result)); }
    let mut index = 0;
    while index < events.len()
        invariant
            index <= events.len(),
            replay_success_prefix(state, events@, index as int, &result),
        decreases events.len() - index,
    {
        let next = apply_working_event(&result, &events[index])?;
        proof {
            assert(event_success_frame(&result, &events@[index as int], &next));
            assert(index as int + 1 > 0);
            assert(index as int + 1 - 1 == index as int);
            assert(exists |previous: WorkingState|
                replay_success_prefix(state, events@, index as int, &previous)
                    && event_success_frame(
                        &previous,
                        &events@[index as int],
                        &next,
                    ));
            reveal_with_fuel(replay_success_prefix, 2);
            assert(replay_success_prefix(state, events@, index as int + 1, &next));
        }
        result = next;
        index += 1;
    }
    Ok(result)
}
}
