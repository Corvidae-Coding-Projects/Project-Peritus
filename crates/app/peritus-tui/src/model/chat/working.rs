//! One ephemeral busy-period timer, driven by monotonic observations from the UI runtime.

use peritus_types::RunId;
use std::time::Instant;

/// Client-observed elapsed time, without inferring provider progress or historical duration.
#[derive(Debug, Default)]
pub struct WorkingIndicator {
    run: Option<RunId>,
    started: Option<Instant>,
    seconds: u64,
}

impl WorkingIndicator {
    pub(crate) fn observe(&mut self, run: Option<RunId>, now: Option<Instant>) {
        if self.run != run {
            self.run = run;
            self.started = None;
            self.seconds = 0;
        }
        if run.is_some()
            && let Some(now) = now
        {
            let started = *self.started.get_or_insert(now);
            self.seconds = self.seconds.max(now.saturating_duration_since(started).as_secs());
        }
    }

    pub(crate) fn elapsed_seconds(&self) -> Option<u64> {
        self.run.map(|_| self.seconds)
    }
}
