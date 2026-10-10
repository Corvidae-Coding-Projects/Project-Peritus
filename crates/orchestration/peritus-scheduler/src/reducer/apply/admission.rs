//! Production work admission with exact rejection and successor contracts.

mod checks;

use crate::state::mutation;
use crate::{SchedulerEventKind, SchedulerPhase, SchedulerState, WorkPhase, WorkRecord, WorkSpec};

#[doc = " Rejection before a work-admission event is produced."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionRejection {
    #[doc = " Draining closes admission."]
    Draining,
    #[doc = " Retention or versioned queue pressure has reached its bound."]
    CapacityLimit,
    #[doc = " The requested work identity is retained."]
    DuplicateWork,
    #[doc = " The work revision differs from the immutable binding."]
    RevisionMismatch,
    #[doc = " The request exceeds global capacity."]
    ResourceConflict,
    #[doc = " A dependency is not retained."]
    MissingDependency,
    #[doc = " The parent is not retained."]
    MissingParent,
    #[doc = " No retained owner worker supports this work."]
    UnsupportedWork,
    #[doc = " The admission ordinal cannot advance."]
    OrdinalOverflow,
}
fn check(state: &SchedulerState, spec: &WorkSpec) -> Option<AdmissionRejection> {
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
#[doc = " Applies admission checks, ordinal advancement, exact insertion and event construction."]
#[doc = ""]
#[doc = " # Errors"]
#[doc = " Returns the first rejected condition while leaving the complete state unchanged."]
pub fn apply_command(
    state: &mut SchedulerState,
    spec: &WorkSpec,
) -> Result<SchedulerEventKind, AdmissionRejection> {
    if let Some(reason) = check(state, spec) {
        return Err(reason);
    }
    let Some(ordinal) = mutation::next_enqueue_ordinal(state) else {
        return Err(AdmissionRejection::OrdinalOverflow);
    };
    let phase = if spec.dependencies().is_empty() {
        WorkPhase::Queued
    } else {
        WorkPhase::WaitingDependencies
    };
    let definition = spec.clone();
    let record = WorkRecord::new(definition, phase, ordinal);
    mutation::insert_work(state, record);
    Ok(SchedulerEventKind::WorkAdmitted { spec: spec.clone() })
}
