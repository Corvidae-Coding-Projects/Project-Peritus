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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn elapsed_time_uses_monotonic_observations_not_the_number_of_ticks() {
        let mut indicator = WorkingIndicator::default();
        let run = RunId::new([1; 16]).expect("run");
        let now = Instant::now();
        indicator.observe(Some(run), Some(now));
        assert_eq!(indicator.elapsed_seconds(), Some(0));
        indicator.observe(Some(run), Some(now + Duration::from_secs(40)));
        assert_eq!(indicator.elapsed_seconds(), Some(40));
        indicator.observe(Some(run), None);
        assert_eq!(indicator.elapsed_seconds(), Some(40));
    }

    #[test]
    fn idle_or_changed_conversations_reset_the_busy_period() {
        let mut indicator = WorkingIndicator::default();
        let first = RunId::new([1; 16]).expect("first run");
        let second = RunId::new([2; 16]).expect("second run");
        let now = Instant::now();
        indicator.observe(Some(first), Some(now));
        indicator.observe(Some(first), Some(now + Duration::from_secs(10)));
        indicator.observe(None, None);
        assert_eq!(indicator.elapsed_seconds(), None);
        indicator.observe(Some(first), None);
        assert_eq!(indicator.elapsed_seconds(), Some(0));
        indicator.observe(Some(first), Some(now + Duration::from_secs(20)));
        indicator.observe(Some(first), Some(now + Duration::from_secs(30)));
        assert_eq!(indicator.elapsed_seconds(), Some(10));
        indicator.observe(Some(second), Some(now + Duration::from_secs(31)));
        assert_eq!(indicator.elapsed_seconds(), Some(0));
    }
}
