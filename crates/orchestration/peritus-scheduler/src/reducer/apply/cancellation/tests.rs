//! Totality and ordering checks over arbitrary finite parent graphs.

use peritus_codec::CodecLimits;

use crate::{
    RecoveryPolicy, SchedulerState, WorkId, WorkPhase, WorkRecord, WorkSpec, WorkTerminal,
    decode_scheduler_state,
};

fn template_state() -> SchedulerState {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fixtures/protocol/scheduler-v1-recovery/retry/checkpoint.bin");
    let bytes = std::fs::read(path).expect("read immutable legacy checkpoint fixture");
    decode_scheduler_state(&bytes, CodecLimits::PRODUCTION)
        .expect("historical scheduler checkpoint supplies an immutable work template")
}

fn work_id(value: u8) -> WorkId {
    WorkId::new([value; 16]).expect("fixed work identity is nonzero")
}

fn record(
    template: &SchedulerState,
    id: u8,
    parent: Option<WorkId>,
    phase: WorkPhase,
) -> WorkRecord {
    let original = template.work()[0].spec();
    let spec = WorkSpec::new(
        work_id(id),
        original.owner(),
        original.revision(),
        original.class(),
        original.priority(),
        original.request().clone(),
        original.budget_reservation(),
        Vec::new(),
        parent,
        original.maximum_attempts(),
        RecoveryPolicy::Fail,
        original.payload_digest(),
        template.binding().limits(),
    )
    .expect("synthetic work preserves the checked immutable fixture fields");
    if phase == WorkPhase::Terminal {
        WorkRecord::from_wire(spec, phase, u64::from(id), 0, 0, None, Some(WorkTerminal::Cancelled))
    } else {
        WorkRecord::new(spec, phase, u64::from(id))
    }
}

#[test]
fn reached_parent_cycle_is_finite_and_selects_the_whole_component() {
    let template = template_state();
    let root = work_id(10);
    let middle = work_id(11);
    let tail = work_id(12);
    let work = vec![
        record(&template, 10, Some(tail), WorkPhase::Queued),
        record(&template, 11, Some(root), WorkPhase::Queued),
        record(&template, 12, Some(middle), WorkPhase::Queued),
    ];

    assert_eq!(super::affected(&work, root, true), vec![root, middle, tail]);
}

#[test]
fn disconnected_parent_cycle_is_not_selected() {
    let template = template_state();
    let root = work_id(10);
    let child = work_id(11);
    let cycle_left = work_id(20);
    let cycle_right = work_id(21);
    let work = vec![
        record(&template, 10, None, WorkPhase::Queued),
        record(&template, 11, Some(root), WorkPhase::Queued),
        record(&template, 20, Some(cycle_right), WorkPhase::Queued),
        record(&template, 21, Some(cycle_left), WorkPhase::Queued),
    ];

    assert_eq!(super::affected(&work, root, true), vec![root, child]);
}

#[test]
fn missing_parent_is_safe_whether_or_not_it_is_the_requested_root() {
    let template = template_state();
    let retained_root = work_id(10);
    let missing = work_id(99);
    let orphan = work_id(30);
    let orphan_child = work_id(31);
    let work = vec![
        record(&template, 10, None, WorkPhase::Queued),
        record(&template, 30, Some(missing), WorkPhase::Queued),
        record(&template, 31, Some(orphan), WorkPhase::Queued),
    ];

    assert_eq!(super::affected(&work, retained_root, true), vec![retained_root]);
    assert_eq!(super::affected(&work, missing, false), Vec::<WorkId>::new());
    assert_eq!(super::affected(&work, missing, true), vec![orphan, orphan_child]);
}

#[test]
fn terminal_ancestor_carries_reachability_but_is_not_selected() {
    let template = template_state();
    let root = work_id(10);
    let terminal = work_id(20);
    let live_grandchild = work_id(30);
    let work = vec![
        record(&template, 10, None, WorkPhase::Queued),
        record(&template, 20, Some(root), WorkPhase::Terminal),
        record(&template, 30, Some(terminal), WorkPhase::Queued),
    ];

    assert_eq!(super::affected(&work, root, true), vec![root, live_grandchild]);
}

#[test]
fn shuffled_input_preserves_the_exact_selected_subsequence() {
    let template = template_state();
    let root = work_id(80);
    let child = work_id(70);
    let grandchild = work_id(60);
    let work = vec![
        record(&template, 60, Some(child), WorkPhase::Queued),
        record(&template, 40, None, WorkPhase::Queued),
        record(&template, 80, None, WorkPhase::Queued),
        record(&template, 70, Some(root), WorkPhase::Queued),
    ];

    assert_eq!(super::affected(&work, root, true), vec![grandchild, root, child]);
}
