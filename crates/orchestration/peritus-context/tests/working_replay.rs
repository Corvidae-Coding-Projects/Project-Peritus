//! Canonical persisted state/event compatibility and deterministic restart replay.

mod working_support;

use peritus_context::working::{
    WorkingBinding, WorkingCodecError, WorkingDelta, WorkingEntryStatus, WorkingError,
    WorkingEvent, WorkingLimits, apply_working_event, decode_working_event, decode_working_state,
    encode_working_event, encode_working_state, refresh_working_state, replay_working_events,
};
use peritus_role::HarnessRole;
use working_support::*;

#[test]
fn canonical_observation_version_one_fixture_is_stable() {
    let event = WorkingEvent::Observation { binding: binding(), source: source(1) };
    let bytes = encode_working_event(&event).unwrap();
    let mut expected = b"PWME\0\x01\0".to_vec();
    expected.extend_from_slice(&[1; 16]);
    expected.extend_from_slice(&[2; 16]);
    expected.extend_from_slice(&[3; 16]);
    expected.push(0); // Writer.
    expected.extend_from_slice(&0_u64.to_be_bytes());
    expected.extend_from_slice(&1_u64.to_be_bytes());
    expected.extend_from_slice(source(1).artifact().as_bytes());
    expected.extend_from_slice(&8_u64.to_be_bytes());
    expected.extend_from_slice(&0_u64.to_be_bytes());
    expected.extend_from_slice(&8_u64.to_be_bytes());
    expected.push(2); // ToolOutput.
    assert_eq!(bytes, expected);
    assert_eq!(decode_working_event(&bytes, binding(), limits()).unwrap(), event);
}

#[test]
fn snapshot_plus_committed_suffix_matches_uninterrupted_execution() {
    let initial = state();
    let event = WorkingEvent::Delta(
        WorkingDelta::new(
            binding(),
            initial.revision(),
            vec![entry(1, vec![obs(1)], vec![], validity(vec![file(10, b"old")]))],
            limits(),
        )
        .unwrap(),
    );
    let populated = apply_working_event(&initial, &event).unwrap();
    let refresh = WorkingEvent::Refresh {
        base_revision: populated.revision(),
        environment: environment(b"new", vec![file(10, b"new")]),
    };
    let observed = WorkingEvent::Observation { binding: binding(), source: source(2) };
    let uninterrupted =
        replay_working_events(&populated, &[refresh.clone(), observed.clone()]).unwrap();
    let snapshot = encode_working_state(&populated).unwrap();
    let restored = decode_working_state(&snapshot, binding(), limits()).unwrap();
    let decoded: Vec<_> = [refresh, observed]
        .iter()
        .map(|e| {
            decode_working_event(&encode_working_event(e).unwrap(), binding(), limits()).unwrap()
        })
        .collect();
    assert_eq!(replay_working_events(&restored, &decoded).unwrap(), uninterrupted);
    assert_eq!(
        decode_working_event(&encode_working_event(&event).unwrap(), binding(), limits()).unwrap(),
        event
    );
}

#[test]
fn snapshot_preserves_sticky_staleness_and_superseded_evidence() {
    let initial = apply(
        &state(),
        vec![
            entry(1, vec![obs(1)], vec![], validity(vec![])),
            entry(2, vec![obs(1)], vec![], validity(vec![file(10, b"old")])),
        ],
    )
    .unwrap();
    let replaced = apply(
        &initial,
        vec![entry(3, vec![obs(1)], vec![], validity(vec![])).with_supersedes(id(1)).unwrap()],
    )
    .unwrap();
    let stale = refresh_working_state(
        &replaced,
        replaced.revision(),
        environment(b"new", vec![file(10, b"new")]),
    )
    .unwrap();
    let restored =
        decode_working_state(&encode_working_state(&stale).unwrap(), binding(), limits()).unwrap();
    assert_eq!(restored, stale);
    assert_eq!(restored.entry(binding(), id(1)).unwrap().status(), WorkingEntryStatus::Superseded);
    assert_eq!(restored.entry(binding(), id(2)).unwrap().status(), WorkingEntryStatus::Stale);
    assert_eq!(
        apply(&restored, vec![entry(2, vec![obs(1)], vec![], validity(vec![]))]),
        Err(WorkingError::StaleEntry)
    );
}

#[test]
fn all_truncated_prefixes_and_trailing_bytes_are_rejected() {
    let event = WorkingEvent::Observation { binding: binding(), source: source(1) };
    let event_bytes = encode_working_event(&event).unwrap();
    for prefix in 0..event_bytes.len() {
        assert!(decode_working_event(&event_bytes[..prefix], binding(), limits()).is_err());
    }
    let bytes = encode_working_state(&state()).unwrap();
    for prefix in 0..bytes.len() {
        assert!(decode_working_state(&bytes[..prefix], binding(), limits()).is_err());
    }
    let mut trailing = bytes;
    trailing.push(0);
    assert!(decode_working_state(&trailing, binding(), limits()).is_err());
}

#[test]
fn unknown_versions_scope_and_wider_snapshot_limits_fail_closed() {
    let state = state();
    let bytes = encode_working_state(&state).unwrap();
    let b = binding();
    let wrong = WorkingBinding::new(b.run(), b.workspace(), b.task(), HarnessRole::Reviewer, 0);
    assert_eq!(
        decode_working_state(&bytes, wrong, limits()),
        Err(WorkingCodecError::State(WorkingError::BindingMismatch))
    );
    let narrow = WorkingLimits::new(1, 1, 1, 1, 1).unwrap();
    assert_eq!(
        decode_working_state(&bytes, b, narrow),
        Err(WorkingCodecError::State(WorkingError::Capacity))
    );
    let mut future = bytes;
    future[5] = 2;
    assert_eq!(decode_working_state(&future, b, limits()), Err(WorkingCodecError::InvalidValue));
}

#[test]
fn replay_never_skips_a_bad_event_or_redispatches_any_operation() {
    let state = state();
    let wrong = WorkingEvent::Observation { binding: binding(), source: source(3) };
    assert_eq!(replay_working_events(&state, &[wrong]), Err(WorkingError::SourceSequence));
    assert_eq!(state.through_observation(), 1);
    let duplicate = WorkingEvent::Observation { binding: binding(), source: source(1) };
    assert_eq!(replay_working_events(&state, &[duplicate]).unwrap(), state);
}
