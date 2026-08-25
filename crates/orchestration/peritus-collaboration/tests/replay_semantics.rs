//! Replay exactness, duplicate rejection, and projection coverage.

#![allow(clippy::unwrap_used, reason = "fixed replay fixtures use checked values")]

mod support;

use peritus_collaboration::{
    CollaborationCommandKind, CollaborationProjection, JoinPolicy, replay, replay_equivalent,
};

use support::{Fixture, apply};

#[test]
fn replay_is_exact_and_rejects_empty_duplicate_and_reordered_streams() {
    assert!(replay(&[]).is_err());
    let fixture = Fixture::new(JoinPolicy::NoChildren);
    let (state, mut events) = fixture.start();
    let state = fixture.activate_root(state, &mut events, 10);
    let state = apply(
        state,
        &mut events,
        11,
        CollaborationCommandKind::Pause { requested_by: fixture.root_owner },
    );
    let rebuilt = replay(&events).unwrap();
    assert!(replay_equivalent(&state, &rebuilt));
    let mut duplicate = events.clone();
    duplicate.push(events[1].clone());
    assert!(replay(&duplicate).is_err());
    let mut reordered = events.clone();
    reordered.swap(1, 2);
    assert!(replay(&reordered).is_err());
    let projection = CollaborationProjection::from_state(&state);
    assert_eq!(projection.tasks().len(), 1);
    assert_eq!(projection.sequence(), 3);
}
