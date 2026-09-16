//! Required closure can exceed a preferred allocation, but never the hard input ceiling.

mod working_support;

use peritus_context::working::{
    WorkingEntry, WorkingEntryKind, WorkingEntryStatus, WorkingError, WorkingLinks,
    render_working_state, render_working_state_with_headroom,
};
use working_support::*;

#[test]
fn headroom_preserves_required_roots_and_shared_dependencies_without_optional_expansion() {
    let plan = WorkingEntry::new(
        id(2),
        WorkingEntryKind::Plan,
        content(b"unfinished plan"),
        WorkingLinks::new(vec![obs(1)], vec![], vec![id(1)], limits()).unwrap(),
        validity(vec![]),
        limits(),
    )
    .unwrap();
    let failed = WorkingEntry::new(
        id(3),
        WorkingEntryKind::FailedApproach,
        content(b"failed experiment"),
        WorkingLinks::new(vec![obs(1)], vec![], vec![id(1)], limits()).unwrap(),
        validity(vec![]),
        limits(),
    )
    .unwrap();
    let contradicted = entry(4, vec![obs(1)], vec![id(2)], validity(vec![]))
        .with_status(WorkingEntryStatus::Contradicted)
        .unwrap();
    let required = apply(
        &state(),
        vec![entry(1, vec![obs(1)], vec![], validity(vec![])), plan, failed, contradicted],
    )
    .unwrap();
    let tokens = render_working_state(&required, binding(), 4096)
        .unwrap()
        .plan()
        .unwrap()
        .accounting()
        .used_input();
    let state = apply(&required, vec![entry(5, vec![obs(1)], vec![], validity(vec![]))]).unwrap();
    let original = state.clone();
    let view = render_working_state_with_headroom(&state, binding(), 1, 4096).unwrap();
    assert_eq!(view.plan().unwrap().accounting().used_input(), tokens);
    assert_eq!(view.plan().unwrap().accounting().usable_input(), tokens);
    let mut ids: Vec<_> = view
        .plan()
        .unwrap()
        .segments()
        .iter()
        .map(peritus_context::RenderSegment::source_id)
        .collect();
    ids.sort();
    assert_eq!(ids, vec![id(1), id(2), id(3), id(4)]);
    assert_eq!(view.omitted(), &[id(5)]);
    assert!(render_working_state_with_headroom(&state, binding(), 4096, tokens).is_ok());
    assert!(matches!(
        render_working_state_with_headroom(&state, binding(), 1, tokens - 1),
        Err(WorkingError::Capacity)
    ));
    assert!(matches!(render_working_state(&state, binding(), 1), Err(WorkingError::Capacity)));
    assert_eq!(state, original);
}

#[test]
fn optional_entries_do_not_borrow_headroom() {
    let state = apply(&state(), vec![entry(1, vec![obs(1)], vec![], validity(vec![]))]).unwrap();
    let view = render_working_state_with_headroom(&state, binding(), 1, 4096).unwrap();
    assert!(view.plan().unwrap().segments().is_empty());
    assert_eq!(view.omitted(), &[id(1)]);
}
