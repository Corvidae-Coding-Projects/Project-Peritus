//! Independent selector traces for priority, aging, feasibility, and stable ties.

#![allow(clippy::unwrap_used, reason = "fixed checked test corpus")]

mod support;

use peritus_scheduler::{
    DispatchId, RecoveryPolicy, SchedulerCommandKind, SchedulerState, WorkPhase, select_next,
};

use support::{Fixture, bytes, digest};

#[allow(
    clippy::too_many_lines,
    reason = "independent reference comparison trace is intentionally contiguous"
)]
#[test]
fn implementation_matches_independent_reference_and_forces_aged_work() {
    let fixture = Fixture::new();
    let (mut state, mut events) = fixture.started();
    Fixture::apply(
        &mut state,
        &mut events,
        3,
        SchedulerCommandKind::RegisterWorker { descriptor: fixture.worker(30, 1) },
    );
    for (command, id, priority) in [(4, 40, 1), (5, 41, 9), (6, 42, 9)] {
        Fixture::apply(
            &mut state,
            &mut events,
            command,
            SchedulerCommandKind::AdmitWork {
                spec: fixture.work(id, priority, Vec::new(), None, 1, RecoveryPolicy::Fail),
            },
        );
    }
    let mut observed = Vec::new();
    for (index, command) in [7_u8, 10].into_iter().enumerate() {
        let index = u8::try_from(index).expect("two-element trace index fits in u8");
        let selected = select_next(&state).unwrap();
        assert_eq!((selected.work_id(), selected.worker_id()), reference_select(&state).unwrap());
        let dispatch = DispatchId::new(bytes(60 + index)).unwrap();
        let transition = Fixture::apply(
            &mut state,
            &mut events,
            command,
            SchedulerCommandKind::DispatchNext {
                dispatch_id: dispatch,
                dispatch_token: digest(70 + index),
            },
        );
        let peritus_scheduler::SchedulerEventKind::WorkReserved { reservation } =
            transition.event().kind()
        else {
            panic!("dispatch command did not produce a reservation")
        };
        observed.push(reservation.work_id());
        Fixture::apply(
            &mut state,
            &mut events,
            command + 1,
            SchedulerCommandKind::AcknowledgeStart { dispatch_id: dispatch },
        );
        Fixture::apply(
            &mut state,
            &mut events,
            command + 2,
            SchedulerCommandKind::CompleteWork {
                dispatch_id: dispatch,
                result_digest: digest(80 + index),
            },
        );
    }
    Fixture::apply(
        &mut state,
        &mut events,
        13,
        SchedulerCommandKind::AdmitWork {
            spec: fixture.work(43, 9, Vec::new(), None, 1, RecoveryPolicy::Fail),
        },
    );
    for (index, command) in [14_u8, 17].into_iter().enumerate() {
        let index = u8::try_from(index).expect("two-element trace index fits in u8");
        let selected = select_next(&state).unwrap();
        assert_eq!((selected.work_id(), selected.worker_id()), reference_select(&state).unwrap());
        let dispatch = DispatchId::new(bytes(62 + index)).unwrap();
        let transition = Fixture::apply(
            &mut state,
            &mut events,
            command,
            SchedulerCommandKind::DispatchNext {
                dispatch_id: dispatch,
                dispatch_token: digest(72 + index),
            },
        );
        let peritus_scheduler::SchedulerEventKind::WorkReserved { reservation } =
            transition.event().kind()
        else {
            panic!("dispatch command did not produce a reservation")
        };
        observed.push(reservation.work_id());
        Fixture::apply(
            &mut state,
            &mut events,
            command + 1,
            SchedulerCommandKind::AcknowledgeStart { dispatch_id: dispatch },
        );
        Fixture::apply(
            &mut state,
            &mut events,
            command + 2,
            SchedulerCommandKind::CompleteWork {
                dispatch_id: dispatch,
                result_digest: digest(82 + index),
            },
        );
    }
    assert_eq!(observed[0].into_bytes(), bytes(41));
    assert_eq!(observed[1].into_bytes(), bytes(42));
    assert_eq!(observed[2].into_bytes(), bytes(40), "low priority item is forced at bypass bound");
    assert_eq!(observed[3].into_bytes(), bytes(43));
}

fn reference_select(
    state: &SchedulerState,
) -> Option<(peritus_scheduler::WorkId, peritus_scheduler::WorkerId)> {
    let limit = state.binding().limits().bypass_count();
    let mut work: Vec<_> = state
        .work()
        .iter()
        .filter(|record| record.phase() == WorkPhase::Queued)
        .filter_map(|record| {
            let worker = state.workers().iter().find(|worker| {
                worker.phase() == peritus_scheduler::WorkerPhase::Available
                    && worker.descriptor().owner() == record.spec().owner()
                    && worker.descriptor().supports(record.spec().class())
                    && record.spec().request().fits_within(worker.descriptor().capacity())
            })?;
            Some((record, worker.descriptor().id()))
        })
        .collect();
    work.sort_by(|(left, _), (right, _)| {
        (right.bypasses() >= limit)
            .cmp(&(left.bypasses() >= limit))
            .then_with(|| right.spec().priority().cmp(&left.spec().priority()))
            .then_with(|| left.enqueue_ordinal().cmp(&right.enqueue_ordinal()))
            .then_with(|| left.spec().id().cmp(&right.spec().id()))
    });
    let (work, worker) = work.first()?;
    Some((work.spec().id(), *worker))
}
