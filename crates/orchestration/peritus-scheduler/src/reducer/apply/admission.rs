//! Production work admission with exact rejection and successor contracts.

#[cfg(verus_only)]
mod bounds;
mod checks;
#[cfg(verus_only)]
mod contracts;

use crate::state::mutation;
use crate::{SchedulerEventKind, SchedulerPhase, SchedulerState, WorkPhase, WorkRecord, WorkSpec};
#[cfg(verus_only)]
use contracts::{
    admission_preserves_other_state, admitted_record, command_matches, preflight_rejection,
};
use vstd::prelude::*;

verus! {

/// Rejection before a work-admission event is produced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionRejection {
    /// Draining closes admission.
    Draining,
    /// Retention or versioned queue pressure has reached its bound.
    CapacityLimit,
    /// The requested work identity is retained.
    DuplicateWork,
    /// The work revision differs from the immutable binding.
    RevisionMismatch,
    /// The request exceeds global capacity.
    ResourceConflict,
    /// A dependency is not retained.
    MissingDependency,
    /// The parent is not retained.
    MissingParent,
    /// No retained owner worker supports this work.
    UnsupportedWork,
    /// The admission ordinal cannot advance.
    OrdinalOverflow,
}

fn check(state: &SchedulerState, spec: &WorkSpec) -> (rejection: Option<AdmissionRejection>)
    ensures state.spec_work_ordered() ==> rejection == preflight_rejection(state, spec),
{
    proof { reveal(preflight_rejection); }
    if matches!(state.phase(), SchedulerPhase::Draining | SchedulerPhase::DrainingPaused) {
        return Some(AdmissionRejection::Draining);
    }
    let limits = state.binding().limits();
    if state.work().len() >= limits.retained_work() as usize
        || crate::state::queue::admission_count(state) >= limits.queued_work() as usize
    {
        return Some(AdmissionRejection::CapacityLimit);
    }
    if checks::contains_work(state, spec.id()) {
        return Some(AdmissionRejection::DuplicateWork);
    }
    if !super::super::fences::revision_values_equal(spec.revision(), state.binding().revision()) {
        return Some(AdmissionRejection::RevisionMismatch);
    }
    if !spec.request().fits_within(state.binding().capacity()) {
        return Some(AdmissionRejection::ResourceConflict);
    }
    if !checks::all_dependencies_retained(state, spec) {
        return Some(AdmissionRejection::MissingDependency);
    }
    if !checks::has_retained_parent(state, spec) {
        return Some(AdmissionRejection::MissingParent);
    }
    if !checks::has_supporting_worker(state, spec) {
        return Some(AdmissionRejection::UnsupportedWork);
    }
    None
}

/// Applies admission checks, ordinal advancement, exact insertion and event construction.
///
/// # Errors
/// Returns the first rejected condition while leaving the complete state unchanged.
pub fn apply_command(
    state: &mut SchedulerState,
    spec: &WorkSpec,
) -> (result: Result<SchedulerEventKind, AdmissionRejection>)
    ensures command_matches(old(state), final(state), spec, &result),
{
    if let Some(reason) = check(state, spec) {
        proof { reveal(command_matches); reveal(contracts::expected_rejection); }
        return Err(reason);
    }
    let ghost was_ordered = state.spec_work_ordered();
    let ghost was_ready = state.spec_reservation_reducer_ready();
    let Some(ordinal) = mutation::next_enqueue_ordinal(state) else {
        proof { reveal(command_matches); reveal(contracts::expected_rejection); }
        return Err(AdmissionRejection::OrdinalOverflow);
    };
    let phase = if spec.dependencies().is_empty() {
        WorkPhase::Queued
    } else {
        WorkPhase::WaitingDependencies
    };
    let definition = spec.clone();
    proof { WorkSpec::clone_admission_fields(spec, &definition); }
    let record = WorkRecord::new(definition, phase, ordinal);
    let ghost admitted = record;
    proof {
        reveal(mutation::enqueue_preserves_other_state);
        reveal(admitted_record);
        if was_ordered {
            reveal(preflight_rejection);
            reveal(checks::retained);
            reveal(crate::verified::work_identity_absent);
            assert(crate::verified::work_identity_absent(
                state.spec_work(), record.spec_definition().spec_id(),
            ));
        }
        if was_ready {
            assert(state.spec_reservation_reducer_ready());
        }
        reveal(crate::verified::work_phase_retains_reservation);
        reveal(SchedulerState::spec_collections_ordered);
    }
    mutation::insert_work(state, record);
    proof {
        reveal(mutation::work_update_preserves_other_state);
        reveal(admission_preserves_other_state);
        reveal(command_matches);
        reveal(contracts::expected_rejection);
        let at = choose |index: int| #![auto]
            0 <= index <= old(state).spec_work().len()
                && state.spec_work() == old(state).spec_work().insert(index, admitted);
        assert(exists |at: int, value: WorkRecord| #![auto]
            0 <= at <= old(state).spec_work().len()
                && admitted_record(value, spec, state.spec_enqueue_ordinal())
                && state.spec_work() == old(state).spec_work().insert(at, value));
        if was_ordered {
            bounds::admission_preserves_bounds(old(state), state, spec, admitted, at);
        }
    }
    Ok(SchedulerEventKind::WorkAdmitted { spec: spec.clone() })
}

} // verus!
