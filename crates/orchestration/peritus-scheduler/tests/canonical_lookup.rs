//! Public admission and live lookup agree with independent linear membership.

mod support;

use peritus_scheduler::{
    DispatchId, ExecutionClass, RecoveryPolicy, SchedulerCommandKind, SchedulerError,
    SchedulerState, WorkId, WorkSpec, WorkerDescriptor, WorkerId, replay,
};
use support::{Fixture, digest};

const fn identity(position: usize, value: u8) -> [u8; 16] {
    let mut bytes = [128; 16];
    bytes[position] = value;
    bytes
}

fn check_lookups(state: &SchedulerState, position: usize) -> Result<(), SchedulerError> {
    assert!(
        state
            .workers()
            .windows(2)
            .all(|pair| { pair[0].descriptor().id() < pair[1].descriptor().id() })
    );
    assert!(state.work().windows(2).all(|pair| pair[0].spec().id() < pair[1].spec().id()));
    assert!(
        state
            .reservations()
            .windows(2)
            .all(|pair| { pair[0].dispatch_id() < pair[1].dispatch_id() })
    );
    assert!(state.used_dispatches().windows(2).all(|pair| pair[0] < pair[1]));
    for value in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 254, 255] {
        let bytes = identity(position, value);
        let worker_id = WorkerId::new(bytes)?;
        let work_id = WorkId::new(bytes)?;
        let dispatch_id = DispatchId::new(bytes)?;
        assert_eq!(
            state.worker(worker_id),
            state.workers().iter().find(|record| record.descriptor().id() == worker_id),
        );
        assert_eq!(
            state.work_item(work_id),
            state.work().iter().find(|record| record.spec().id() == work_id),
        );
        assert_eq!(
            state.reservation(dispatch_id),
            state.reservations().iter().find(|record| record.dispatch_id() == dispatch_id),
        );
    }
    Ok(())
}

#[test]
fn admission_and_dispatch_lookups_cover_every_identity_byte_and_search_boundary()
-> Result<(), SchedulerError> {
    for position in 0..16 {
        let fixture = Fixture::new();
        let (mut state, mut events) = fixture.started();
        let worker_template = fixture.worker(30, 1);
        let work_template = fixture.work(40, 1, Vec::new(), None, 1, RecoveryPolicy::Fail);
        let mut command = 3;
        let mut values = [6, 0, 255, 2, 10, 4, 8];
        let rotation = position % values.len();
        values.rotate_left(rotation);
        check_lookups(&state, position)?;

        for value in values {
            let descriptor = WorkerDescriptor::new(
                WorkerId::new(identity(position, value))?,
                fixture.owner,
                vec![ExecutionClass::Tool],
                worker_template.capacity().clone(),
                1,
                fixture.limits,
            )?;
            Fixture::apply(
                &mut state,
                &mut events,
                command,
                SchedulerCommandKind::RegisterWorker { descriptor },
            );
            command += 1;
            check_lookups(&state, position)?;
        }

        for value in values.into_iter().rev() {
            let spec = WorkSpec::new(
                WorkId::new(identity(position, value))?,
                fixture.owner,
                fixture.binding.revision(),
                ExecutionClass::Tool,
                1,
                work_template.request().clone(),
                None,
                Vec::new(),
                None,
                work_template.maximum_attempts(),
                RecoveryPolicy::Fail,
                digest(value),
                fixture.limits,
            )?;
            Fixture::apply(
                &mut state,
                &mut events,
                command,
                SchedulerCommandKind::AdmitWork { spec },
            );
            command += 1;
            check_lookups(&state, position)?;
        }

        for value in values {
            Fixture::apply(
                &mut state,
                &mut events,
                command,
                SchedulerCommandKind::DispatchNext {
                    dispatch_id: DispatchId::new(identity(position, value))?,
                    dispatch_token: digest(value),
                },
            );
            command += 1;
            check_lookups(&state, position)?;
        }

        for value in values.into_iter().rev() {
            Fixture::apply(
                &mut state,
                &mut events,
                command,
                SchedulerCommandKind::AcknowledgeStart {
                    dispatch_id: DispatchId::new(identity(position, value))?,
                },
            );
            command += 1;
            check_lookups(&state, position)?;
        }
        let history = state.used_dispatches().to_vec();
        for value in values {
            Fixture::apply(
                &mut state,
                &mut events,
                command,
                SchedulerCommandKind::CompleteWork {
                    dispatch_id: DispatchId::new(identity(position, value))?,
                    result_digest: digest(value),
                },
            );
            command += 1;
            check_lookups(&state, position)?;
            assert_eq!(state.used_dispatches(), history);
        }
        assert!(state.reservations().is_empty());
        let reconstructed = replay(&events)?;
        assert_eq!(reconstructed, state);
        check_lookups(&reconstructed, position)?;
    }
    Ok(())
}
