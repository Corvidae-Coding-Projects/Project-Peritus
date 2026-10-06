//! Exact publication crash boundaries for checkpoint regression fixtures.

use super::Error;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotFaultPoint {
    BeforeChunkFinalization,
    BeforeRootPublication,
    AfterRootPublication,
}
static FAULTS: std::sync::Mutex<Vec<(std::thread::ThreadId, SnapshotFaultPoint)>> =
    std::sync::Mutex::new(Vec::new());
pub fn inject_snapshot_fault(point: SnapshotFaultPoint) {
    let thread = std::thread::current().id();
    let mut faults = FAULTS.lock().expect("snapshot fault registry");
    faults.retain(|(owner, _)| *owner != thread);
    faults.push((thread, point));
}
pub(super) fn check(point: SnapshotFaultPoint) -> Result<(), Error> {
    let thread = std::thread::current().id();
    let mut faults = FAULTS.lock().expect("snapshot fault registry");
    if let Some(index) =
        faults.iter().position(|(owner, fault)| *owner == thread && *fault == point)
    {
        faults.swap_remove(index);
        return Err(std::io::Error::other("injected snapshot publication interruption").into());
    }
    Ok(())
}
