//! Exact root admission and event construction for production cancellation.

use crate::{SchedulerEventKind, SchedulerState, WorkId, WorkPhase};

#[doc = " Rejections emitted before any cancellation mutation."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancellationRejection {
    #[doc = " The requested work identity is not retained."]
    WorkNotRetained,
    #[doc = " The requested work is already terminal."]
    WorkAlreadyTerminal,
}
#[doc = " Runs root admission, the actual cancellation update and exact event construction."]
#[doc = ""]
#[doc = " # Errors"]
#[doc = " Rejects an absent or already-terminal root before changing state."]
pub fn apply_command(
    state: &mut SchedulerState,
    root: WorkId,
    descendants: bool,
) -> Result<SchedulerEventKind, CancellationRejection> {
    let Some(retained) = state.work_item(root) else {
        return Err(CancellationRejection::WorkNotRetained);
    };
    let terminal = retained.phase().same(WorkPhase::Terminal);
    if terminal {
        return Err(CancellationRejection::WorkAlreadyTerminal);
    }
    let (affected, _) = super::cancel_retained(state, root, descendants);
    Ok(SchedulerEventKind::WorkCancelled { work_id: root, descendants, affected })
}
