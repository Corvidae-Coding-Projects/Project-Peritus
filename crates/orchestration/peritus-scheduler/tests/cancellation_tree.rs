//! Cancellation-tree closure and active-ownership regression traces.

#[path = "cancellation_tree/oracle.rs"]
mod oracle;
mod support;

use std::collections::BTreeSet;

use peritus_scheduler::{
    DispatchId, RecoveryPolicy, SchedulerCommandKind, SchedulerDirective, SchedulerErrorKind,
    SchedulerEvent, SchedulerReservation, SchedulerState, WorkId, WorkPhase, WorkTerminal, decide,
    pending_directives,
};

use oracle::{
    assert_cancel_event, assert_history_roundtrip, dispatch_id, expected_affected, work_id,
};
use support::{Fixture, digest};

#[test]
fn reverse_order_tree_crosses_terminal_ancestor_and_preserves_other_branches() {
    let fixture = Fixture::new();
    let (mut state, mut events) = fixture.started();
    Fixture::apply(
        &mut state,
        &mut events,
        20,
        SchedulerCommandKind::RegisterWorker { descriptor: fixture.worker(30, 1) },
    );
    let super_root = work_id(90);
    let target = work_id(80);
    let sibling = work_id(85);
    let terminal_child = work_id(70);
    let sibling_child = work_id(75);
    let grandchild = work_id(60);
    let great_grandchild = work_id(50);
    let unrelated = work_id(40);

    for (command, id, parent) in [
        (3, 90, None),
        (4, 80, Some(super_root)),
        (5, 85, Some(super_root)),
        (6, 70, Some(target)),
        (7, 75, Some(sibling)),
        (8, 60, Some(terminal_child)),
        (9, 50, Some(grandchild)),
        (10, 40, None),
    ] {
        Fixture::apply(
            &mut state,
            &mut events,
            command,
            SchedulerCommandKind::AdmitWork {
                spec: fixture.work(id, 1, Vec::new(), parent, 1, RecoveryPolicy::Fail),
            },
        );
    }

    let child_cancel = Fixture::apply(
        &mut state,
        &mut events,
        11,
        SchedulerCommandKind::CancelWork { work_id: terminal_child },
    );
    assert_cancel_event(child_cancel.event(), terminal_child, false, vec![terminal_child]);
    assert!(matches!(
        state.work_item(terminal_child).expect("terminal child remains retained").terminal(),
        Some(WorkTerminal::Cancelled)
    ));

    let untouched: Vec<_> = [super_root, sibling, sibling_child, unrelated]
        .into_iter()
        .map(|id| (id, state.work_item(id).expect("unrelated work remains retained").clone()))
        .collect();
    let terminal_before =
        state.work_item(terminal_child).expect("terminal child remains retained").clone();
    let expected = expected_affected(&state, target, true);
    assert_eq!(expected, vec![great_grandchild, grandchild, target]);

    let tree_cancel = Fixture::apply(
        &mut state,
        &mut events,
        12,
        SchedulerCommandKind::CancelWorkTree { work_id: target },
    );
    assert_cancel_event(tree_cancel.event(), target, true, expected);

    for id in [great_grandchild, grandchild, target] {
        assert!(matches!(
            state.work_item(id).expect("cancelled descendant remains retained").terminal(),
            Some(WorkTerminal::Cancelled)
        ));
    }
    assert_eq!(
        state.work_item(terminal_child).expect("terminal child remains retained"),
        &terminal_before
    );
    for (id, before) in untouched {
        assert_eq!(state.work_item(id).expect("unrelated work remains retained"), &before);
    }
    assert_history_roundtrip(&state, &events);
}

#[test]
fn single_cancel_is_root_only_and_rejections_leave_state_unchanged() {
    let fixture = Fixture::new();
    let (mut state, mut events) = fixture.started();
    Fixture::apply(
        &mut state,
        &mut events,
        20,
        SchedulerCommandKind::RegisterWorker { descriptor: fixture.worker(30, 1) },
    );
    let root = work_id(80);
    let child = work_id(70);
    let grandchild = work_id(60);

    for (command, id, parent) in [(3, 80, None), (4, 70, Some(root)), (5, 60, Some(child))] {
        Fixture::apply(
            &mut state,
            &mut events,
            command,
            SchedulerCommandKind::AdmitWork {
                spec: fixture.work(id, 1, Vec::new(), parent, 1, RecoveryPolicy::Fail),
            },
        );
    }

    let child_before = state.work_item(child).expect("child is retained").clone();
    let grandchild_before = state.work_item(grandchild).expect("grandchild is retained").clone();
    let expected = expected_affected(&state, root, false);
    assert_eq!(expected, vec![root]);
    let cancel = Fixture::apply(
        &mut state,
        &mut events,
        6,
        SchedulerCommandKind::CancelWork { work_id: root },
    );
    assert_cancel_event(cancel.event(), root, false, expected);
    assert_eq!(state.work_item(child).expect("child is retained"), &child_before);
    assert_eq!(state.work_item(grandchild).expect("grandchild is retained"), &grandchild_before);

    let before_terminal_rejection = state.clone();
    let terminal =
        Fixture::command(&state, 7, SchedulerCommandKind::CancelWorkTree { work_id: root });
    assert_eq!(
        decide(&state, &terminal).expect_err("terminal root is rejected").kind(),
        SchedulerErrorKind::IllegalTransition
    );
    assert_eq!(state, before_terminal_rejection);

    let before_unknown_rejection = state.clone();
    let unknown =
        Fixture::command(&state, 8, SchedulerCommandKind::CancelWorkTree { work_id: work_id(99) });
    assert_eq!(
        decide(&state, &unknown).expect_err("unknown root is rejected").kind(),
        SchedulerErrorKind::UnknownIdentity
    );
    assert_eq!(state, before_unknown_rejection);
    assert_history_roundtrip(&state, &events);
}

struct ActiveTree {
    state: SchedulerState,
    events: Vec<SchedulerEvent>,
    root: WorkId,
    reserved_work: WorkId,
    running_work: WorkId,
    cancelling_work: WorkId,
    reserved_dispatch: DispatchId,
    running_dispatch: DispatchId,
    cancelling_dispatch: DispatchId,
}

fn active_tree_fixture() -> ActiveTree {
    let fixture = Fixture::new();
    let (mut state, mut events) = fixture.started();
    let root = work_id(80);
    let reserved_work = work_id(70);
    let running_work = work_id(60);
    let cancelling_work = work_id(50);
    let reserved_dispatch = dispatch_id(120);
    let running_dispatch = dispatch_id(121);
    let cancelling_dispatch = dispatch_id(122);

    Fixture::apply(
        &mut state,
        &mut events,
        3,
        SchedulerCommandKind::RegisterWorker { descriptor: fixture.worker(30, 4) },
    );
    admit_active_tree(&fixture, &mut state, &mut events, root, reserved_work, running_work);
    reserve_active_tree(
        &mut state,
        &mut events,
        [reserved_dispatch, running_dispatch, cancelling_dispatch],
    );
    assert_dispatch_owners(
        &state,
        [reserved_dispatch, running_dispatch, cancelling_dispatch],
        [reserved_work, running_work, cancelling_work],
    );
    Fixture::apply(
        &mut state,
        &mut events,
        11,
        SchedulerCommandKind::AcknowledgeStart { dispatch_id: running_dispatch },
    );
    Fixture::apply(
        &mut state,
        &mut events,
        12,
        SchedulerCommandKind::CancelWork { work_id: cancelling_work },
    );
    assert_active_phases(&state, reserved_work, running_work, cancelling_work);
    ActiveTree {
        state,
        events,
        root,
        reserved_work,
        running_work,
        cancelling_work,
        reserved_dispatch,
        running_dispatch,
        cancelling_dispatch,
    }
}

fn admit_active_tree(
    fixture: &Fixture,
    state: &mut SchedulerState,
    events: &mut Vec<SchedulerEvent>,
    root: WorkId,
    reserved_work: WorkId,
    running_work: WorkId,
) {
    for (command, id, priority, parent) in [
        (4, 80, 1, None),
        (5, 70, 9, Some(root)),
        (6, 60, 8, Some(reserved_work)),
        (7, 50, 7, Some(running_work)),
    ] {
        Fixture::apply(
            state,
            events,
            command,
            SchedulerCommandKind::AdmitWork {
                spec: fixture.work(id, priority, Vec::new(), parent, 1, RecoveryPolicy::Fail),
            },
        );
    }
}

fn reserve_active_tree(
    state: &mut SchedulerState,
    events: &mut Vec<SchedulerEvent>,
    dispatches: [DispatchId; 3],
) {
    for (command, dispatch) in (8_u8..=10).zip(dispatches) {
        Fixture::apply(
            state,
            events,
            command,
            SchedulerCommandKind::DispatchNext {
                dispatch_id: dispatch,
                dispatch_token: digest(dispatch.into_bytes()[0]),
            },
        );
    }
}

fn assert_dispatch_owners(
    state: &SchedulerState,
    dispatches: [DispatchId; 3],
    expected_work: [WorkId; 3],
) {
    for (dispatch, work) in dispatches.into_iter().zip(expected_work) {
        assert_eq!(state.reservation(dispatch).expect("active dispatch exists").work_id(), work);
    }
}

fn assert_active_phases(
    state: &SchedulerState,
    reserved_work: WorkId,
    running_work: WorkId,
    cancelling_work: WorkId,
) {
    for (id, phase) in [
        (reserved_work, WorkPhase::Reserved),
        (running_work, WorkPhase::Running),
        (cancelling_work, WorkPhase::Cancelling),
    ] {
        assert_eq!(state.work_item(id).expect("active work exists").phase(), phase);
    }
}

fn request_active_tree_cancellation(tree: &mut ActiveTree) -> Vec<SchedulerReservation> {
    let reservations_before = tree.state.reservations().to_vec();
    let resources_before = tree.state.used_resources().expect("resource sum is valid");
    let expected = expected_affected(&tree.state, tree.root, true);
    assert_eq!(
        expected,
        vec![tree.cancelling_work, tree.running_work, tree.reserved_work, tree.root]
    );
    let tree_cancel = Fixture::apply(
        &mut tree.state,
        &mut tree.events,
        13,
        SchedulerCommandKind::CancelWorkTree { work_id: tree.root },
    );
    assert_cancel_event(tree_cancel.event(), tree.root, true, expected);
    assert_eq!(tree.state.reservations(), reservations_before.as_slice());
    assert_eq!(tree.state.used_resources().expect("resource sum remains valid"), resources_before);
    for id in [tree.cancelling_work, tree.running_work, tree.reserved_work] {
        assert_eq!(
            tree.state.work_item(id).expect("active descendant remains retained").phase(),
            WorkPhase::Cancelling
        );
    }
    assert!(matches!(
        tree.state.work_item(tree.root).expect("root remains retained").terminal(),
        Some(WorkTerminal::Cancelled)
    ));
    assert_eq!(
        pending_cancellations(&tree.state),
        BTreeSet::from([tree.reserved_dispatch, tree.running_dispatch, tree.cancelling_dispatch,])
    );
    reservations_before
}

fn pending_cancellations(state: &SchedulerState) -> BTreeSet<DispatchId> {
    pending_directives(state)
        .into_iter()
        .filter_map(|directive| match directive {
            SchedulerDirective::Cancel { dispatch_id, .. } => Some(dispatch_id),
            SchedulerDirective::Dispatch(_) => None,
        })
        .collect()
}

fn assert_success_rejected(
    tree: &ActiveTree,
    command_id: u8,
    result_digest: u8,
    expected: SchedulerErrorKind,
) {
    let before = tree.state.clone();
    let command = Fixture::command(
        &tree.state,
        command_id,
        SchedulerCommandKind::CompleteWork {
            dispatch_id: tree.running_dispatch,
            result_digest: digest(result_digest),
        },
    );
    assert_eq!(
        decide(&tree.state, &command).expect_err("cancelled work cannot succeed").kind(),
        expected
    );
    assert_eq!(tree.state, before);
}

fn acknowledge_active_tree(tree: &mut ActiveTree) {
    for (command, dispatch) in
        [(15, tree.reserved_dispatch), (16, tree.running_dispatch), (17, tree.cancelling_dispatch)]
    {
        Fixture::apply(
            &mut tree.state,
            &mut tree.events,
            command,
            SchedulerCommandKind::AcknowledgeCancellation { dispatch_id: dispatch },
        );
    }
    assert!(tree.state.reservations().is_empty());
    assert_eq!(tree.state.used_resources().expect("empty resource sum is valid"), None);
    for id in [tree.cancelling_work, tree.running_work, tree.reserved_work, tree.root] {
        assert!(matches!(
            tree.state.work_item(id).expect("cancelled work remains retained").terminal(),
            Some(WorkTerminal::Cancelled)
        ));
    }
}

#[test]
fn active_descendants_keep_ownership_until_each_cancellation_is_acknowledged() {
    let mut tree = active_tree_fixture();
    let reservations_before = request_active_tree_cancellation(&mut tree);
    assert_success_rejected(&tree, 14, 99, SchedulerErrorKind::IllegalTransition);
    assert_eq!(tree.state.reservations(), reservations_before.as_slice());

    acknowledge_active_tree(&mut tree);
    assert_success_rejected(&tree, 18, 100, SchedulerErrorKind::UnknownIdentity);
    assert_history_roundtrip(&tree.state, &tree.events);
}
