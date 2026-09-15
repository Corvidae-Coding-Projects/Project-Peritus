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
            state.spec_observation_result(*source, next)
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
            &&& (next.spec_revision() == state.spec_revision() ==>
                super::WorkingEntry::sequence_clone_equivalent(
                    state.spec_entries(),
                    next.spec_entries(),
                ))
            &&& (next.spec_revision() > state.spec_revision() ==>
                WorkingState::spec_entry_statuses(next.spec_entries())
                    == WorkingState::spec_invalidated_entry_statuses(
                        state.spec_entries(),
                        environment,
                        state.spec_observations().len() as u64,
                    ))
            &&& next.spec_limits() == state.spec_limits()
            &&& next.spec_protocol().spec_requirements()
                == state.spec_protocol().spec_requirements()
            &&& next.spec_protocol().spec_pending()
                == state.spec_protocol().spec_pending()
        }
        WorkingEvent::Delta(delta) => {
            &&& next.spec_environment().spec_binding()
                == state.spec_environment().spec_binding()
            &&& next.spec_environment().spec_candidate()
                == state.spec_environment().spec_candidate()
            &&& next.spec_environment().spec_files()
                == state.spec_environment().spec_files()
            &&& next.spec_revision() as int == state.spec_revision() as int + 1
            &&& next.spec_observations() == state.spec_observations()
            &&& delta.spec_result_entries(state, next.spec_entries())
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

/// Event-family-specific error relation used to identify replay's first failing event.
pub open spec fn event_error_frame(
    event: &WorkingEvent,
    error: WorkingError,
) -> bool {
    match event {
        WorkingEvent::Observation { .. } => error.spec_is_observation_error(),
        WorkingEvent::Refresh { .. } => error.spec_is_refresh_error(),
        WorkingEvent::Delta(_) => error.spec_is_delta_error(),
        WorkingEvent::Protocol(_) => error.spec_is_protocol_error(),
    }
}

/// The capacity precheck or first event error after an exact successful prefix.
pub open spec fn replay_first_error(
    initial: &WorkingState,
    events: Seq<WorkingEvent>,
    error: WorkingError,
) -> bool {
    if events.len() > initial.spec_limits().spec_observations() {
        error == WorkingError::Capacity
    } else {
        exists |index: int, current: WorkingState|
            #[trigger] replay_error_witness(initial, events, index, &current, error)
    }
}

/// One failing event following an exact successful replay prefix.
pub open spec fn replay_error_witness(
    initial: &WorkingState,
    events: Seq<WorkingEvent>,
    index: int,
    current: &WorkingState,
    error: WorkingError,
) -> bool {
    &&& 0 <= index < events.len()
    &&& replay_success_prefix(initial, events, index, current)
    &&& event_error_frame(&events[index], error)
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
#[allow(clippy::too_many_lines, reason = "each event arm discharges its exact result contract")]
pub fn apply_working_event(
    state: &WorkingState,
    event: &WorkingEvent,
) -> (result: Result<WorkingState, WorkingError>)
    ensures match result {
        Ok(next) => event_success_frame(state, event, &next),
        Err(error) => event_error_frame(event, error),
    },
{
    proof { reveal(event_success_frame); }
    match event {
        WorkingEvent::Observation { binding, source } => {
            match ingest_working_observation(state, *binding, *source) {
                Ok(next) => {
                    proof {
                        assert(state.spec_observation_result(*source, &next));
                        assert(event_success_frame(state, event, &next));
                    }
                    Ok(next)
                }
                Err(error) => {
                    proof { assert(event_error_frame(event, error)); }
                    Err(error)
                }
            }
        }
        WorkingEvent::Refresh { base_revision, environment } => {
            let refreshed_environment = environment.clone();
            proof {
                assert(refreshed_environment.spec_binding() == environment.spec_binding());
                assert(refreshed_environment.spec_candidate() == environment.spec_candidate());
                assert(refreshed_environment.spec_files() == environment.spec_files());
            }
            match refresh_working_state(state, *base_revision, refreshed_environment) {
                Ok(next) => {
                    proof {
                        assert(next.spec_environment().spec_binding()
                            == refreshed_environment.spec_binding());
                        assert(next.spec_environment().spec_binding()
                            == environment.spec_binding());
                        assert(next.spec_environment().spec_candidate()
                            == environment.spec_candidate());
                        assert(next.spec_environment().spec_files()
                            == environment.spec_files());
                        assert(next.spec_revision() >= state.spec_revision());
                        assert(next.spec_revision() as int
                            <= state.spec_revision() as int + 1);
                        assert(next.spec_observations() == state.spec_observations());
                        assert(super::WorkingEntry::sequence_payload_equivalent(
                            state.spec_entries(), next.spec_entries(),
                        ));
                        if next.spec_revision() == state.spec_revision() {
                            assert(super::WorkingEntry::sequence_clone_equivalent(
                                state.spec_entries(), next.spec_entries(),
                            ));
                        }
                        if next.spec_revision() > state.spec_revision() {
                            assert(WorkingState::spec_entry_statuses(next.spec_entries())
                                == WorkingState::spec_invalidated_entry_statuses(
                                    state.spec_entries(),
                                    &refreshed_environment,
                                    state.spec_observations().len() as u64,
                                ));
                            super::invalidation_environment::invalidation_result_environment_equivalent(
                                state.spec_entries(),
                                &refreshed_environment,
                                environment,
                                state.spec_observations().len() as u64,
                            );
                            WorkingState::invalidated_entry_statuses_definition(
                                state.spec_entries(),
                                &refreshed_environment,
                                state.spec_observations().len() as u64,
                            );
                            WorkingState::invalidated_entry_statuses_definition(
                                state.spec_entries(),
                                environment,
                                state.spec_observations().len() as u64,
                            );
                            assert(WorkingState::spec_entry_statuses(next.spec_entries())
                                == WorkingState::spec_invalidated_entry_statuses(
                                    state.spec_entries(),
                                    environment,
                                    state.spec_observations().len() as u64,
                                ));
                        }
                        assert(next.spec_limits() == state.spec_limits());
                        assert(next.spec_protocol().spec_requirements()
                            == state.spec_protocol().spec_requirements());
                        assert(next.spec_protocol().spec_pending()
                            == state.spec_protocol().spec_pending());
                        reveal(event_success_frame);
                        assert(event_success_frame(state, event, &next));
                    }
                    Ok(next)
                }
                Err(error) => {
                    proof { assert(event_error_frame(event, error)); }
                    Err(error)
                }
            }
        }
        WorkingEvent::Delta(delta) => {
            match apply_working_delta(state, delta) {
                Ok(next) => {
                    proof { assert(event_success_frame(state, event, &next)); }
                    Ok(next)
                }
                Err(error) => {
                    proof { assert(event_error_frame(event, error)); }
                    Err(error)
                }
            }
        }
        WorkingEvent::Protocol(update) => {
            match super::apply_working_protocol(state, update) {
                Ok(next) => {
                    proof { assert(event_success_frame(state, event, &next)); }
                    Ok(next)
                }
                Err(error) => {
                    proof { assert(event_error_frame(event, error)); }
                    Err(error)
                }
            }
        }
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
        Err(error) => replay_first_error(state, events@, error),
    },
{
    let limits = state.limits();
    let maximum_events = limits.observations();
    proof {
        assert(limits == state.spec_limits());
        assert(maximum_events as nat == limits.spec_observations());
        assert(maximum_events as nat == state.spec_limits().spec_observations());
    }
    if events.len() > maximum_events {
        proof {
            assert(events@.len() > state.spec_limits().spec_observations());
            assert(replay_first_error(state, events@, WorkingError::Capacity));
        }
        return Err(WorkingError::Capacity);
    }
    let mut result = state.clone();
    proof {
        assert(events@.len() <= state.spec_limits().spec_observations());
        assert(replay_success_prefix(state, events@, 0, &result));
    }
    let mut index = 0;
    while index < events.len()
        invariant
            index <= events.len(),
            events@.len() <= state.spec_limits().spec_observations(),
            replay_success_prefix(state, events@, index as int, &result),
        decreases events.len() - index,
    {
        let next = match apply_working_event(&result, &events[index]) {
            Ok(next) => next,
            Err(error) => {
                proof {
                    assert(events@.len() <= state.spec_limits().spec_observations());
                    assert(event_error_frame(&events@[index as int], error));
                    assert(replay_error_witness(
                        state,
                        events@,
                        index as int,
                        &result,
                        error,
                    ));
                    assert(exists |failed_index: int, current: WorkingState|
                        replay_error_witness(
                            state,
                            events@,
                            failed_index,
                            &current,
                            error,
                        ));
                    assert(replay_first_error(state, events@, error));
                }
                return Err(error);
            }
        };
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
