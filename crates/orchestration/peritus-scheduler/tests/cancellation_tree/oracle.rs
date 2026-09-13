//! Independent cancellation-tree oracle and shared assertions.

use std::collections::BTreeSet;

use peritus_codec::CodecLimits;
use peritus_scheduler::{
    DispatchId, SchedulerEvent, SchedulerEventKind, SchedulerState, WorkId, WorkPhase,
    decode_scheduler_state, encode_scheduler_state, replay,
};

use super::support::bytes;

pub fn work_id(value: u8) -> WorkId {
    WorkId::new(bytes(value)).expect("fixed work identity is nonzero")
}

pub fn dispatch_id(value: u8) -> DispatchId {
    DispatchId::new(bytes(value)).expect("fixed dispatch identity is nonzero")
}

/// Computes reachability independently by walking each candidate's parent chain.
fn reaches_root(state: &SchedulerState, candidate: WorkId, root: WorkId) -> bool {
    let mut cursor = candidate;
    let mut visited = BTreeSet::new();
    loop {
        if cursor == root {
            return true;
        }
        if !visited.insert(cursor) {
            return false;
        }
        let Some(parent) = state.work_item(cursor).and_then(|record| record.spec().parent()) else {
            return false;
        };
        cursor = parent;
    }
}

pub fn expected_affected(state: &SchedulerState, root: WorkId, descendants: bool) -> Vec<WorkId> {
    state
        .work()
        .iter()
        .filter(|record| {
            record.phase() != WorkPhase::Terminal
                && if descendants {
                    reaches_root(state, record.spec().id(), root)
                } else {
                    record.spec().id() == root
                }
        })
        .map(|record| record.spec().id())
        .collect()
}

pub fn assert_cancel_event(
    event: &SchedulerEvent,
    root: WorkId,
    descendants: bool,
    expected: Vec<WorkId>,
) {
    assert_eq!(
        event.kind(),
        &SchedulerEventKind::WorkCancelled { work_id: root, descendants, affected: expected }
    );
}

pub fn assert_history_roundtrip(state: &SchedulerState, events: &[SchedulerEvent]) {
    let rebuilt = replay(events).expect("accepted cancellation history replays");
    assert_eq!(&rebuilt, state);

    let bytes = encode_scheduler_state(state, CodecLimits::PRODUCTION)
        .expect("cancellation checkpoint encodes");
    let decoded = decode_scheduler_state(&bytes, CodecLimits::PRODUCTION)
        .expect("cancellation checkpoint decodes");
    assert_eq!(&decoded, state);
}
