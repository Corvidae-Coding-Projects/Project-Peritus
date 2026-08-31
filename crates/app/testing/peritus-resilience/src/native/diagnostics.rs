//! Bounded diagnostic bytes retained from one native controller.

use std::sync::{Arc, Mutex, MutexGuard};

const MAX_DIAGNOSTIC_BYTES: usize = 16 * 1024;

#[derive(Clone, Default)]
pub(super) struct Diagnostics {
    bytes: Arc<Mutex<Vec<u8>>>,
}

impl Diagnostics {
    pub(super) fn record(&self, bytes: &[u8]) {
        let mut retained = lock(&self.bytes);
        let available = MAX_DIAGNOSTIC_BYTES.saturating_sub(retained.len());
        retained.extend_from_slice(&bytes[..bytes.len().min(available)]);
    }

    pub(super) fn render(&self) -> String {
        String::from_utf8_lossy(&lock(&self.bytes)).trim().to_owned()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
