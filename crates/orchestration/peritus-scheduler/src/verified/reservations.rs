//! Actual-state reservation capacity and ownership model for OBL-0149.

use crate::SchedulerState;

mod arithmetic;
mod clone;
mod entity_insertion;
mod feasibility;
mod insertion;
mod preservation;
mod readiness;
mod removal;
mod work_update;
mod worker_update;

impl SchedulerState {}
