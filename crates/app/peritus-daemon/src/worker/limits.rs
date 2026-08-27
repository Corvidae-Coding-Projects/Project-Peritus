//! Positive worker, result, reap, and shutdown bounds.

use std::{num::NonZeroUsize, time::Duration};

use super::{WorkerSupervisorError, WorkerSupervisorErrorKind};

const MAXIMUM_TASKS: usize = 4_096;
const MAXIMUM_SHUTDOWN: Duration = Duration::from_mins(10);

/// Complete operational bounds for one worker supervisor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkerSupervisorLimits {
    maximum_active_tasks: NonZeroUsize,
    maximum_results: NonZeroUsize,
    maximum_reap_per_pass: NonZeroUsize,
    shutdown_grace: Duration,
    abort_join_grace: Duration,
}

impl WorkerSupervisorLimits {
    /// Creates checked positive worker and shutdown bounds.
    ///
    /// # Errors
    ///
    /// Rejects zero, oversized, inverted, or unreasonably long limits.
    pub(crate) fn new(
        maximum_active_tasks: usize,
        maximum_results: usize,
        maximum_reap_per_pass: usize,
        shutdown_grace: Duration,
        abort_join_grace: Duration,
    ) -> Result<Self, WorkerSupervisorError> {
        let maximum_active_tasks = NonZeroUsize::new(maximum_active_tasks).ok_or_else(invalid)?;
        let maximum_results = NonZeroUsize::new(maximum_results).ok_or_else(invalid)?;
        let maximum_reap_per_pass = NonZeroUsize::new(maximum_reap_per_pass).ok_or_else(invalid)?;
        if maximum_active_tasks.get() > MAXIMUM_TASKS
            || maximum_results.get() > MAXIMUM_TASKS
            || maximum_results < maximum_active_tasks
            || maximum_reap_per_pass > maximum_active_tasks
            || shutdown_grace.is_zero()
            || abort_join_grace.is_zero()
            || shutdown_grace > MAXIMUM_SHUTDOWN
            || abort_join_grace > MAXIMUM_SHUTDOWN
        {
            return Err(invalid());
        }
        Ok(Self {
            maximum_active_tasks,
            maximum_results,
            maximum_reap_per_pass,
            shutdown_grace,
            abort_join_grace,
        })
    }

    /// Builds balanced bounds from the configured daemon task ceiling.
    ///
    /// # Errors
    ///
    /// Rejects a zero or oversized active-task ceiling or invalid shutdown duration.
    pub(crate) fn for_active_tasks(
        maximum_active_tasks: usize,
        shutdown_grace: Duration,
    ) -> Result<Self, WorkerSupervisorError> {
        Self::new(
            maximum_active_tasks,
            maximum_active_tasks,
            maximum_active_tasks.min(64),
            shutdown_grace,
            Duration::from_secs(5),
        )
    }

    pub(super) const fn maximum_active_tasks(self) -> usize {
        self.maximum_active_tasks.get()
    }
    pub(super) const fn maximum_results(self) -> usize {
        self.maximum_results.get()
    }
    pub(super) const fn maximum_reap_per_pass(self) -> usize {
        self.maximum_reap_per_pass.get()
    }
    pub(super) const fn shutdown_grace(self) -> Duration {
        self.shutdown_grace
    }
    pub(super) const fn abort_join_grace(self) -> Duration {
        self.abort_join_grace
    }
}

const fn invalid() -> WorkerSupervisorError {
    WorkerSupervisorError::new(
        WorkerSupervisorErrorKind::InvalidLimit,
        "worker supervisor limits are zero, inverted, or outside production bounds",
    )
}
