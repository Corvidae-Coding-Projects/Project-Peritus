//! Bounded complete process-tree cleanup before terminal publication.

use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use crate::{ProcessError, ProcessEventKind, control::SharedObservation, platform::PlatformProcess};

use super::{POLL_MILLIS, SupervisorPlan, elapsed_millis, emit};

pub(super) fn ensure_tree_quiescent(
    process: &mut dyn PlatformProcess,
    reap_millis: Option<u64>,
    forced: &mut bool,
    shared: &Arc<SharedObservation>,
    plan: &SupervisorPlan,
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
    while reap_millis.is_none_or(|maximum| elapsed_millis(began) < maximum) {
        if process.tree_quiescent()? {
            return Ok(true);
        }
        thread::sleep(Duration::from_millis(POLL_MILLIS));
    }
    process.tree_quiescent()
}
