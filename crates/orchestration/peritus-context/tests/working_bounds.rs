//! Working-state allocation, canonical proposal, and repeated invalidation boundaries.

mod working_support;

use peritus_context::working::{
    WorkingDelta, WorkingEntryStatus, WorkingError, WorkingLimits, WorkingState,
    apply_working_delta, ingest_working_observation, refresh_working_state,
};
use working_support::*;

#[test]
fn reviewer_projection_omits_even_required_derived_entries_and_rejects_other_roles() {
    use peritus_context::working::{
        WorkingBinding, WorkingEntry, WorkingEntryKind, WorkingEnvironment, WorkingLinks,
        render_working_state,
    };
    use peritus_role::HarnessRole;
    let writer = binding();
    let reviewer = WorkingBinding::new(
        writer.run(),
        writer.workspace(),
        writer.task(),
        HarnessRole::Reviewer,
        0,
    );
    let environment =
        WorkingEnvironment::new(reviewer, peritus_codec::sha256(b"candidate"), vec![], limits())
            .unwrap();
    let state = WorkingState::new(environment, limits()).unwrap();
    let state = ingest_working_observation(&state, reviewer, source(1)).unwrap();
    let entry = WorkingEntry::new(
        id(1),
        WorkingEntryKind::FailedApproach,
        content(b"failure"),
        WorkingLinks::new(vec![obs(1)], vec![], vec![], limits()).unwrap(),
        validity(vec![]),
        limits(),
    )
    .unwrap();
    let state = apply(&state, vec![entry]).unwrap();
    let view = render_working_state(&state, reviewer, 1).unwrap();
    assert!(view.plan().is_none());
    assert_eq!(view.omitted(), &[id(1)]);
    assert!(render_working_state(&state, writer, 4096).is_err());
}

#[test]
fn stale_evidence_must_postdate_repeated_environment_invalidation() {
    let initial =
        apply(&state(), vec![entry(1, vec![obs(1)], vec![], validity(vec![file(10, b"old")]))])
            .unwrap();
    let stale = refresh_working_state(
        &initial,
        initial.revision(),
        environment(b"second", vec![file(10, b"second")]),
    )
    .unwrap();
    let observed = ingest_working_observation(&stale, binding(), source(2)).unwrap();
    let changed_again = refresh_working_state(
        &observed,
        observed.revision(),
        environment(b"third", vec![file(10, b"third")]),
    )
    .unwrap();
    let proposal = entry(1, vec![obs(2)], vec![], validity(vec![file(10, b"third")]));
    assert_eq!(apply(&changed_again, vec![proposal]), Err(WorkingError::StaleEntry));
    assert_eq!(changed_again.entry(binding(), id(1)).unwrap().status(), WorkingEntryStatus::Stale);
}

#[test]
fn source_capacity_exhaustion_keeps_the_existing_prefix() {
    let narrow = WorkingLimits::new(1, 16, 256, 8, 8).unwrap();
    let empty = WorkingState::new(environment(b"initial", vec![]), narrow).unwrap();
    let full = ingest_working_observation(&empty, binding(), source(1)).unwrap();
    assert_eq!(
        ingest_working_observation(&full, binding(), source(2)),
        Err(WorkingError::Capacity)
    );
    assert_eq!(full.through_observation(), 1);
    assert_eq!(ingest_working_observation(&full, binding(), source(1)).unwrap(), full);
}

#[test]
fn state_rechecks_proposals_built_with_wider_limits() {
    let narrow = WorkingLimits::new(32, 1, 256, 8, 8).unwrap();
    let empty = WorkingState::new(environment(b"initial", vec![]), narrow).unwrap();
    let state = ingest_working_observation(&empty, binding(), source(1)).unwrap();
    let proposals = vec![
        entry(1, vec![obs(1)], vec![], validity(vec![])),
        entry(2, vec![obs(1)], vec![], validity(vec![])),
    ];
    assert_eq!(apply(&state, proposals), Err(WorkingError::Capacity));
    assert!(state.entries(binding()).unwrap().is_empty());

    let narrow_content = WorkingLimits::new(32, 16, 1, 8, 8).unwrap();
    let empty = WorkingState::new(environment(b"initial", vec![]), narrow_content).unwrap();
    let state = ingest_working_observation(&empty, binding(), source(1)).unwrap();
    assert_eq!(
        apply(&state, vec![entry(1, vec![obs(1)], vec![], validity(vec![]))]),
        Err(WorkingError::Capacity)
    );
}

#[test]
fn duplicate_unordered_and_host_status_proposals_are_rejected() {
    let one = entry(1, vec![obs(1)], vec![], validity(vec![]));
    let two = entry(2, vec![obs(1)], vec![], validity(vec![]));
    assert_eq!(
        WorkingDelta::new(binding(), 1, vec![one.clone(), one.clone()], limits()),
        Err(WorkingError::NonCanonicalOrder)
    );
    assert_eq!(
        WorkingDelta::new(binding(), 1, vec![two, one], limits()),
        Err(WorkingError::NonCanonicalOrder)
    );
    let current =
        apply(&state(), vec![entry(1, vec![obs(1)], vec![], validity(vec![file(10, b"old")]))])
            .unwrap();
    let stale =
        refresh_working_state(&current, current.revision(), environment(b"new", vec![])).unwrap();
    assert_eq!(
        WorkingDelta::new(
            binding(),
            stale.revision(),
            vec![stale.entry(binding(), id(1)).unwrap().clone()],
            limits()
        ),
        Err(WorkingError::DerivedStatus)
    );
}

#[test]
fn identical_inputs_reduce_to_identical_state_and_noop_refresh_is_idempotent() {
    let state = state();
    let delta = WorkingDelta::new(
        binding(),
        state.revision(),
        vec![entry(1, vec![obs(1)], vec![], validity(vec![]))],
        limits(),
    )
    .unwrap();
    assert_eq!(
        apply_working_delta(&state, &delta).unwrap(),
        apply_working_delta(&state, &delta).unwrap()
    );
    assert_eq!(
        refresh_working_state(&state, state.revision(), state.environment().clone()).unwrap(),
        state
    );
}
