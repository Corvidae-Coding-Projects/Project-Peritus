//! Bounded idempotency-window integration tests.

mod support;

use peritus_app_protocol::{
    CommandDisposition, CommandResult, CommittedEventRange, EventCursor, IdempotencyAdmission,
    IdempotencyRecordDisposition, IdempotencyWindow,
};
use support::command_binding;

#[test]
fn window_distinguishes_new_replay_conflict_capacity_and_removal() {
    let original = command_binding(50, b"shared-key");
    let conflicting = command_binding(51, b"shared-key");
    let next = command_binding(52, b"next-key");
    let range = CommittedEventRange::new(EventCursor::new(10), EventCursor::new(12))
        .expect("positive contiguous range");
    let result = CommandResult::committed(original.request_id(), range);
    let mut window = IdempotencyWindow::new(1).expect("positive capacity");

    assert_eq!(window.admit(&original), IdempotencyAdmission::New);
    assert_eq!(
        window.record(&original, result.clone()).expect("record final result"),
        IdempotencyRecordDisposition::Stored,
    );
    assert_eq!(
        window.record(&original, result).expect("same final result is idempotent"),
        IdempotencyRecordDisposition::AlreadyRecorded,
    );
    match window.admit(&original) {
        IdempotencyAdmission::Replay { original_request_id, result } => {
            assert_eq!(original_request_id, original.request_id());
            assert_eq!(result.disposition(), CommandDisposition::Replayed);
            assert_eq!(result.committed_events(), Some(range));
        }
        other => panic!("expected replay, got {other:?}"),
    }
    assert!(matches!(
        window.admit(&conflicting),
        IdempotencyAdmission::Conflict { original_request_id }
            if original_request_id == original.request_id()
    ));
    assert_eq!(window.admit(&next), IdempotencyAdmission::Capacity);

    let retired = window.retire_oldest().expect("oldest entry exists");
    assert_eq!(retired.original_request_id(), original.request_id());
    assert!(window.is_empty());
    assert_eq!(window.admit(&next), IdempotencyAdmission::New);
    assert_eq!(
        window
            .record(&next, CommandResult::committed(next.request_id(), range))
            .expect("freed capacity accepts a new key"),
        IdempotencyRecordDisposition::Stored,
    );
    assert_eq!(window.len(), 1);
}
