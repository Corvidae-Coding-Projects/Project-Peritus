//! Direct-folder accounting without recursive directory inventory.
use super::{
    BTreeSet, Duration, Instant, ProductRunProgress, ProductRunnerError, RunAccounting,
    RunResourceProbe, validate_run_horizon,
};
impl RunAccounting {
    /// Accounts provider, time, and process memory without a recursive directory inventory.
    pub(crate) fn direct_folder(max_elapsed: Duration) -> Result<Self, ProductRunnerError> {
        validate_run_horizon(max_elapsed)?;
        Ok(Self {
            started: Instant::now(),
            max_elapsed,
            progress: ProductRunProgress::default(),
            response_usage: peritus_agent::DeveloperUsage::default(),
            resources: RunResourceProbe::process_only(),
            unavailable_providers: BTreeSet::new(),
        })
    }
}
