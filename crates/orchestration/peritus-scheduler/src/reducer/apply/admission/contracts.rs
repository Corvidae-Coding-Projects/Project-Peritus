//! Independent admission priority and exact successor-state relations.

use super::{AdmissionRejection, checks};
use crate::{SchedulerEventKind, SchedulerPhase, SchedulerState, WorkPhase, WorkRecord, WorkSpec};
use vstd::prelude::*;

verus! {

pub open spec fn preflight_rejection(
    state: &SchedulerState,
    spec: &WorkSpec,
) -> Option<AdmissionRejection> {
    if matches!(state.spec_phase(), SchedulerPhase::Draining | SchedulerPhase::DrainingPaused) {
        Some(AdmissionRejection::Draining)
    } else if state.spec_work().len() >= state.spec_binding().spec_limits().spec_retained_work()
        || crate::state::queue::admission_pressure(state)
            >= state.spec_binding().spec_limits().spec_queued_work()
    {
        Some(AdmissionRejection::CapacityLimit)
    } else if checks::retained(state, spec.spec_id()) {
        Some(AdmissionRejection::DuplicateWork)
    } else if !super::super::super::fences::revisions_match(
        spec.spec_revision(), state.spec_binding().spec_revision(),
    ) {
        Some(AdmissionRejection::RevisionMismatch)
    } else if !spec.spec_request().spec_fits_within(state.spec_binding().spec_capacity()) {
        Some(AdmissionRejection::ResourceConflict)
    } else if !checks::dependencies_retained(state, spec) {
        Some(AdmissionRejection::MissingDependency)
    } else if !checks::parent_retained(state, spec) {
        Some(AdmissionRejection::MissingParent)
    } else if !checks::supporting_worker_exists(state, spec) {
        Some(AdmissionRejection::UnsupportedWork)
    } else {
        None
    }
}

pub open spec fn expected_rejection(
    state: &SchedulerState,
    spec: &WorkSpec,
) -> Option<AdmissionRejection> {
    match preflight_rejection(state, spec) {
        Some(reason) => Some(reason),
        None => if state.spec_enqueue_ordinal() == u64::MAX {
            Some(AdmissionRejection::OrdinalOverflow)
        } else {
            None
        },
    }
}

pub open spec fn admission_preserves_other_state(
    before: &SchedulerState,
    after: &SchedulerState,
) -> bool {
    &&& after.spec_binding() == before.spec_binding()
    &&& after.spec_phase() == before.spec_phase()
    &&& after.spec_sequence() == before.spec_sequence()
    &&& after.spec_last_event_id() == before.spec_last_event_id()
    &&& after.spec_state_digest() == before.spec_state_digest()
    &&& after.spec_workers() == before.spec_workers()
    &&& after.spec_reservations() == before.spec_reservations()
    &&& after.spec_used_dispatches() == before.spec_used_dispatches()
    &&& after.spec_dispatch_ordinal() == before.spec_dispatch_ordinal()
    &&& after.spec_used_commands() == before.spec_used_commands()
    &&& after.spec_terminal() == before.spec_terminal()
}

pub open spec fn admitted_record(record: WorkRecord, spec: &WorkSpec, ordinal: u64) -> bool {
    &&& WorkSpec::clone_equivalent(spec, record.spec_definition())
    &&& record.spec_phase() == if spec.spec_dependencies().len() == 0 {
        WorkPhase::Queued
    } else {
        WorkPhase::WaitingDependencies
    }
    &&& record.spec_enqueue_ordinal() == ordinal
    &&& record.spec_bypasses() == 0
    &&& record.spec_attempts_started() == 0
    &&& record.spec_retry_cause().is_none()
    &&& record.spec_terminal().is_none()
}

pub open spec fn command_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    spec: &WorkSpec,
    result: &Result<SchedulerEventKind, AdmissionRejection>,
) -> bool {
    match result {
        Err(reason) => {
            &&& *after == *before
            &&& before.spec_work_ordered() ==> expected_rejection(before, spec) == Some(*reason)
        },
        Ok(SchedulerEventKind::WorkAdmitted { spec: emitted }) => {
            &&& WorkSpec::clone_equivalent(spec, emitted)
            &&& before.spec_work_ordered() ==> expected_rejection(before, spec).is_none()
            &&& after.spec_enqueue_ordinal() == before.spec_enqueue_ordinal() + 1
            &&& after.spec_work().len() == before.spec_work().len() + 1
            &&& before.spec_enqueue_ordinal() == before.spec_work().len()
                ==> after.spec_enqueue_ordinal() == after.spec_work().len()
            &&& admission_preserves_other_state(before, after)
            &&& exists |at: int, record: WorkRecord| #![auto]
                0 <= at <= before.spec_work().len()
                    && admitted_record(record, spec, after.spec_enqueue_ordinal())
                    && after.spec_work() == before.spec_work().insert(at, record)
            &&& before.spec_reservation_reducer_ready() && before.spec_work_ordered()
                ==> after.spec_reservation_reducer_ready()
            &&& before.spec_collections_ordered() ==> after.spec_collections_ordered()
            &&& before.spec_work_ordered() ==> {
                &&& after.spec_work().len() <= after.spec_binding().spec_limits().spec_retained_work()
                &&& crate::state::queue::admission_pressure(after)
                    == crate::state::queue::admission_pressure(before) + 1
                &&& crate::state::queue::admission_pressure(after)
                    <= after.spec_binding().spec_limits().spec_queued_work()
                &&& before.spec_reservation_reducer_ready() ==> crate::state::queue::queue_bound(after)
            }
        },
        Ok(_) => false,
    }
}

} // verus!
