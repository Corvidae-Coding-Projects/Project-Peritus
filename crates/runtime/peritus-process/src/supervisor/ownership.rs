//! Bounded complete process-tree cleanup before terminal publication.

use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use crate::{
    ExecutionPlan, ProcessError, ProcessEventKind, control::SharedObservation,
    platform::PlatformProcess,
};

use super::{POLL_MILLIS, elapsed_millis, emit};

pub(super) fn ensure_tree_quiescent(
    process: &mut dyn PlatformProcess,
    reap_millis: u64,
    forced: &mut bool,
    shared: &Arc<SharedObservation>,
    plan: &ExecutionPlan,
) -> Result<bool, ProcessError> {
    if process.tree_quiescent()? {
        return Ok(true);
    }
    if let Err(error) = process.force_kill() {
        if process.tree_quiescent()? {
            return Ok(true);
        }
        return Err(error);
    }
    if !*forced {
        *forced = true;
        emit(shared, plan, None, ProcessEventKind::Escalated, Vec::new());
    }
    let began = Instant::now();
    while elapsed_millis(began) < reap_millis {
        if process.tree_quiescent()? {
            return Ok(true);
        }
        thread::sleep(Duration::from_millis(POLL_MILLIS));
    }
    process.tree_quiescent()
}
