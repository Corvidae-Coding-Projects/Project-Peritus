//! Direct-folder accounting without recursive directory inventory.
use super::{
    AccountingState, Arc, AtomicBool, Duration, Instant, ProductRunnerError, RunAccounting,
    RunResourceProbe, validate_run_horizon,
};
impl RunAccounting {
    /// Accounts provider, time, and process memory without a recursive directory inventory.
    pub(crate) fn direct_folder(max_elapsed: Option<Duration>) -> Result<Self, ProductRunnerError> {
        Self::direct_folder_with_cancellation(max_elapsed, Arc::new(AtomicBool::new(false)))
    }

    pub(crate) fn direct_folder_with_cancellation(
        max_elapsed: Option<Duration>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self, ProductRunnerError> {
        validate_run_horizon(max_elapsed)?;
        Ok(Self {
            started: Instant::now(),
            selected_horizon: max_elapsed,
            state: AccountingState::default(),
            resources: RunResourceProbe::process_only(cancelled),
            pending_failure: None,
        })
    }
}
