//! Publication barriers exist only in unit-test builds, never in the shipped runner.

use serde::{Deserialize, Serialize};
use std::{io::Read as _, path::PathBuf, sync::Mutex, thread::ThreadId};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(in crate::candidate::managed) enum Stage {
    Prepared,
    Source,
    Index,
    Head,
    Archive,
    Completed,
}

static BARRIER: Mutex<Option<(ThreadId, Stage, PathBuf)>> = Mutex::new(None);

pub(super) fn install(stage: Stage, path: PathBuf) {
    *BARRIER.lock().unwrap() = Some((std::thread::current().id(), stage, path));
}

pub(in crate::candidate::managed) fn pause(stage: Stage) {
    let barrier = BARRIER.lock().unwrap().clone();
    if let Some((thread, wanted, path)) = barrier
        && thread == std::thread::current().id()
        && wanted == stage
    {
        std::fs::write(path, format!("{stage:?}\n")).unwrap();
        let mut byte = [0_u8; 1];
        std::io::stdin().read_exact(&mut byte).unwrap();
    }
}
