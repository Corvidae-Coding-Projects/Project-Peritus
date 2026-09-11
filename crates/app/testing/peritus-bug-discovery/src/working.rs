//! Structure-aware working-state checks use public construction and codec boundaries.

use std::collections::BTreeSet;

use peritus_codec::sha256;
use peritus_context::working::{
    ObservationId, ObservationKind, ObservationSource, WorkingBinding, WorkingDelta, WorkingEntry,
    WorkingEntryKind, WorkingEntryStatus, WorkingEnvironment, WorkingLimits, WorkingLinks,
    WorkingRenderView, WorkingState, WorkingValidity, apply_working_delta, decode_working_state,
    encode_working_state, ingest_working_observation, render_working_state_with_headroom,
};
use peritus_context::{ContextLimits, ContextNodeId, bind_context_content};
use peritus_role::HarnessRole;
use peritus_types::{RunId, WorkspaceId};

const ENTRY_LIMIT: usize = 8;

/// Checks generated working state, canonical persistence, and capacity monotonicity.
pub fn check(bytes: &[u8]) {
    let (state, binding, limits, count) = structured_state(bytes);
    check_codec(bytes, &state, binding, limits, count);
    check_render(bytes, &state, binding, count);
}

fn structured_state(bytes: &[u8]) -> (WorkingState, WorkingBinding, WorkingLimits, usize) {
    let limits = WorkingLimits::new(16, ENTRY_LIMIT, 128, ENTRY_LIMIT, ENTRY_LIMIT)
        .expect("fixed discovery limits");
    let binding = WorkingBinding::new(
        RunId::new([1; 16]).expect("run identity"),
        WorkspaceId::new([2; 16]).expect("workspace identity"),
        ContextNodeId::new([3; 16]).expect("task identity"),
        HarnessRole::Writer,
        u64::from(bytes.first().copied().unwrap_or(0)),
    );
    let environment = WorkingEnvironment::new(binding, sha256(bytes), Vec::new(), limits)
        .expect("bounded environment");
    let mut state = WorkingState::new(environment, limits).expect("bounded state");
    let count = usize::from(bytes.get(1).copied().unwrap_or(0)) % ENTRY_LIMIT + 1;
    for sequence in 1..=count {
        let sequence = u64::try_from(sequence).expect("entry limit fits u64");
        let source = ObservationSource::new(
            ObservationId::new(sequence).expect("positive observation"),
            sha256(&[u8::try_from(sequence).expect("entry limit fits u8")]),
            1,
            0,
            1,
            ObservationKind::ToolOutput,
        )
        .expect("bounded observation");
        state = ingest_working_observation(&state, binding, source)
            .expect("contiguous observation sequence");
    }

    let context_limits =
        ContextLimits::new(ENTRY_LIMIT, 128, ENTRY_LIMIT, 5).expect("fixed context limits");
    for index in 0..count {
        let id = node_id(index);
        let selector = bytes
            .get(index + 2)
            .copied()
            .unwrap_or_else(|| u8::try_from(index).expect("entry limit fits u8"));
        let length = usize::from(selector % 64) + 1;
        let content_bytes = (0..length)
            .map(|offset| bytes.get(index + offset + 3).copied().unwrap_or(selector))
            .collect::<Vec<_>>();
        let content =
            bind_context_content(content_bytes.clone(), sha256(&content_bytes), context_limits)
                .expect("bounded content");
        let dependencies =
            if index > 0 && selector & 1 == 1 { vec![node_id(index - 1)] } else { Vec::new() };
        let links = WorkingLinks::new(
            vec![
                ObservationId::new(u64::try_from(index + 1).expect("entry limit fits u64"))
                    .expect("positive observation"),
            ],
            Vec::new(),
            dependencies,
            limits,
        )
        .expect("canonical links");
        let kind = match selector % 4 {
            0 => WorkingEntryKind::Plan,
            1 => WorkingEntryKind::FailedApproach,
            2 => WorkingEntryKind::Hypothesis,
            _ => WorkingEntryKind::Decision,
        };
        let entry = WorkingEntry::new(
            id,
            kind,
            content,
            links,
            WorkingValidity::new(None, None, Vec::new(), limits).expect("bounded validity"),
            limits,
        )
        .expect("bounded entry");
        let delta = WorkingDelta::new(binding, state.revision(), vec![entry], limits)
            .expect("canonical proposal");
        state = apply_working_delta(&state, &delta).expect("valid DAG");
    }
    (state, binding, limits, count)
}

fn check_codec(
    bytes: &[u8],
    state: &WorkingState,
    binding: WorkingBinding,
    limits: WorkingLimits,
    count: usize,
) {
    let encoded = encode_working_state(state).expect("encode canonical state");
    let decoded = decode_working_state(&encoded, binding, limits).expect("decode canonical state");
    assert_eq!(&decoded, state, "working checkpoint round-trip differs");

    if let Some(prefix) = bytes.get(count + 3).map(|value| usize::from(*value) % encoded.len()) {
        assert!(
            decode_working_state(&encoded[..prefix], binding, limits).is_err(),
            "truncated checkpoint was accepted",
        );
    }
    if !encoded.is_empty() {
        let mut changed = encoded;
        let position = usize::from(bytes.get(count + 4).copied().unwrap_or(0)) % changed.len();
        changed[position] ^= 1;
        if let Ok(canonical) = decode_working_state(&changed, binding, limits) {
            assert_eq!(
                encode_working_state(&canonical).expect("re-encode changed canonical state"),
                changed,
                "decoder accepted a noncanonical checkpoint representation",
            );
        }
    }
}

fn check_render(bytes: &[u8], state: &WorkingState, binding: WorkingBinding, count: usize) {
    let preferred = u64::from(bytes.get(count + 5).copied().unwrap_or(0)) + 1;
    let available = preferred + u64::from(bytes.get(count + 6).copied().unwrap_or(0));
    let original = state.clone();
    let low = render_working_state_with_headroom(state, binding, preferred, available);
    let high = render_working_state_with_headroom(state, binding, preferred, available + 1);
    if let Ok(view) = &low {
        assert!(high.is_ok(), "one more available token made a feasible required closure fail");
        assert_render_oracle(state, binding, view, preferred, available);
    }
    assert_eq!(state, &original, "working-state rendering mutated retained state");
}

fn assert_render_oracle(
    state: &WorkingState,
    binding: WorkingBinding,
    view: &WorkingRenderView,
    preferred: u64,
    available: u64,
) {
    let entries = state.entries(binding).expect("matching working binding");
    let selected: BTreeSet<_> = view
        .plan()
        .into_iter()
        .flat_map(peritus_context::RenderPlan::segments)
        .map(peritus_context::RenderSegment::source_id)
        .collect();
    let mut required: BTreeSet<_> = entries
        .iter()
        .filter(|entry| {
            entry.status() != WorkingEntryStatus::Superseded
                && (entry.status() == WorkingEntryStatus::Contradicted
                    || !entry.links().contradicts().is_empty()
                    || matches!(
                        entry.kind(),
                        WorkingEntryKind::FailedApproach | WorkingEntryKind::Plan
                    ) && entry.status() != WorkingEntryStatus::Resolved)
        })
        .map(WorkingEntry::id)
        .collect();
    loop {
        let before = required.len();
        let dependencies: Vec<_> = entries
            .iter()
            .filter(|entry| required.contains(&entry.id()))
            .flat_map(|entry| entry.links().depends_on().iter().copied())
            .collect();
        required.extend(dependencies);
        if required.len() == before {
            break;
        }
    }
    assert!(required.is_subset(&selected), "successful plan omitted a required dependency closure");
    let expected_omitted: Vec<_> =
        entries.iter().map(WorkingEntry::id).filter(|id| !selected.contains(id)).collect();
    assert_eq!(view.omitted(), expected_omitted);
    if let Some(plan) = view.plan() {
        let accounting = plan.accounting();
        assert!(accounting.used_input() <= accounting.usable_input());
        assert!(accounting.context_window() <= available);
        assert!(accounting.context_window() >= preferred.min(available));
    }
}

fn node_id(index: usize) -> ContextNodeId {
    let mut bytes = [0_u8; 16];
    bytes[15] = u8::try_from(index + 1).expect("entry limit fits u8");
    ContextNodeId::new(bytes).expect("nonzero node identity")
}
