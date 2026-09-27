use super::StorageFaultPoint;
use std::{
    io,
    sync::Mutex,
    thread::{self, ThreadId},
};

struct ScheduledFault {
    point: StorageFaultPoint,
    occurrence: u64,
    calls: u64,
    hit: bool,
    kind: io::ErrorKind,
}

static FAULTS: Mutex<Vec<(ThreadId, ScheduledFault)>> = Mutex::new(Vec::new());

pub(crate) fn schedule(point: StorageFaultPoint, occurrence: u64, kind: io::ErrorKind) {
    let thread = thread::current().id();
    let mut faults = FAULTS.lock().expect("storage fault lock");
    assert!(
        !faults.iter().any(|(candidate, _)| *candidate == thread),
        "storage fault already scheduled on this test thread",
    );
    faults.push((thread, ScheduledFault { point, occurrence, calls: 0, hit: false, kind }));
}

pub(super) fn check(point: StorageFaultPoint) -> io::Result<()> {
    let thread = thread::current().id();
    let triggered =
        trigger(FAULTS.lock().expect("storage fault lock").as_mut_slice(), thread, point);
    if let Some(kind) = triggered {
        return Err(io::Error::from(kind));
    }
    Ok(())
}

fn trigger(
    faults: &mut [(ThreadId, ScheduledFault)],
    thread: ThreadId,
    point: StorageFaultPoint,
) -> Option<io::ErrorKind> {
    let (_, scheduled) = faults.iter_mut().find(|(candidate, _)| *candidate == thread)?;
    if scheduled.point != point {
        return None;
    }
    scheduled.calls = scheduled.calls.checked_add(1).expect("fault call count");
    if scheduled.calls != scheduled.occurrence {
        return None;
    }
    scheduled.hit = true;
    Some(scheduled.kind)
}

pub(crate) fn verify_hit() {
    assert!(take_scheduled().hit, "scheduled storage fault was not reached");
}

pub(crate) fn verify_missed() {
    assert!(!take_scheduled().hit, "storage fault unexpectedly triggered");
}

fn take_scheduled() -> ScheduledFault {
    let thread = thread::current().id();
    let mut faults = FAULTS.lock().expect("storage fault lock");
    let index = faults
        .iter()
        .position(|(candidate, _)| *candidate == thread)
        .expect("scheduled storage fault");
    faults.swap_remove(index).1
}
