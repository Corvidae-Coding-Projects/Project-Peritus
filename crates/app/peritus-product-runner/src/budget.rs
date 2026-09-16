//! Cumulative accounting and generous hard ceilings for one complete product run.

use std::{
    collections::BTreeSet,
    path::Path,
    time::{Duration, Instant},
};

use peritus_agent::{DeveloperAccountingEvent, DeveloperUsage};
use peritus_types::ProviderProfileId;

use crate::{ProductRunnerError, ProductRunnerErrorKind};

mod folder;
#[path = "resource_probe.rs"]
mod resource_probe;

use resource_probe::RunResourceProbe;

/// Maximum wall-clock duration of one uninterrupted product-run attempt.
pub const PRODUCT_RUN_MAX_ELAPSED: Duration = Duration::from_hours(8);
use crate::accounting::{
    AccountingError, AccountingState, BudgetViolation, UsageSnapshot, WorkEvent,
};
pub use crate::accounting::{
    PRODUCT_RUN_MAX_COST_MICROUNITS, PRODUCT_RUN_MAX_MODEL_REQUESTS,
    PRODUCT_RUN_MAX_PEAK_RSS_BYTES, PRODUCT_RUN_MAX_TOOL_CALLS, PRODUCT_RUN_MAX_TOTAL_TOKENS,
    PRODUCT_RUN_MAX_WORKSPACE_GROWTH_BYTES, ProductRunProgress,
};

pub struct RunAccounting {
    started: Instant,
    max_elapsed: Duration,
    state: AccountingState,
    resources: RunResourceProbe,
    unavailable_providers: BTreeSet<ProviderProfileId>,
}

impl RunAccounting {
    pub fn new(workspace_root: &Path, max_elapsed: Duration) -> Result<Self, ProductRunnerError> {
        validate_run_horizon(max_elapsed)?;
        Ok(Self {
            started: Instant::now(),
            max_elapsed,
            state: AccountingState::default(),
            resources: RunResourceProbe::new(workspace_root)?,
            unavailable_providers: BTreeSet::new(),
        })
    }

    pub(crate) fn record_event(
        &mut self,
        event: DeveloperAccountingEvent,
    ) -> Result<(), ProductRunnerError> {
        match event {
            DeveloperAccountingEvent::ModelRequest { retry } => {
                self.record_work(WorkEvent::ModelRequest { retry })?;
            }
            DeveloperAccountingEvent::ToolCall => {
                self.record_work(WorkEvent::ToolCall)?;
            }
            DeveloperAccountingEvent::Compaction => {
                self.record_work(WorkEvent::Compaction)?;
            }
            DeveloperAccountingEvent::Usage(counters) => {
                let mut usage = DeveloperUsage::default();
                usage.observe(counters).map_err(|_| exhausted("provider usage overflowed"))?;
                self.record_usage(usage)?;
            }
        }
        // Per-event admission is cheap: do not recursively probe the workspace for every token
        // or tool. The ordinary role/settlement boundary still samples host resources.
        self.state.progress.elapsed_millis = millis(self.started.elapsed());
        budget_violation(self.state.progress, self.started.elapsed(), self.max_elapsed)
            .map_or(Ok(()), |detail| Err(exhausted(detail)))
    }

    fn record_usage(&mut self, usage: DeveloperUsage) -> Result<(), ProductRunnerError> {
        let current = UsageSnapshot {
            input_tokens: usage.input_tokens(),
            cached_input_tokens: usage.cached_input_tokens(),
            output_tokens: usage.output_tokens(),
            total_tokens: usage.total_tokens(),
            provider_cost_microunits: usage.provider_cost_microunits(),
            observations: usage.observations(),
        };
        self.state.apply_usage(current).map_err(|error| match error {
            AccountingError::CounterOverflow => exhausted("run accounting counter overflowed"),
            AccountingError::ObservationOverflow => {
                exhausted("run usage observation counter overflowed")
            }
        })
    }

    fn record_work(&mut self, event: WorkEvent) -> Result<(), ProductRunnerError> {
        if self.state.apply_work(event) {
            Ok(())
        } else {
            Err(exhausted("run accounting counter overflowed"))
        }
    }

    pub(crate) fn record_role_retry(&mut self) -> Result<(), ProductRunnerError> {
        self.record_work(WorkEvent::RoleRetry)?;
        self.check()
    }

    pub fn record_provider_failover(&mut self) -> Result<(), ProductRunnerError> {
        self.record_work(WorkEvent::ProviderFailover)?;
        self.check()
    }

    pub fn open_provider_circuit(&mut self, profile: ProviderProfileId) {
        self.unavailable_providers.insert(profile);
    }

    pub fn provider_circuit_open(&self, profile: ProviderProfileId) -> bool {
        self.unavailable_providers.contains(&profile)
    }

    pub fn close_provider_circuit(&mut self, profile: ProviderProfileId) {
        self.unavailable_providers.remove(&profile);
    }

    pub fn check(&mut self) -> Result<(), ProductRunnerError> {
        self.state.progress.elapsed_millis = millis(self.started.elapsed());
        let resources = self.resources.observe()?;
        self.state.progress.workspace_bytes = resources.workspace;
        self.state.progress.workspace_growth_bytes = resources.growth;
        self.state.progress.peak_rss_bytes =
            self.state.progress.peak_rss_bytes.max(resources.peak_rss);
        let violation =
            budget_violation(self.state.progress, self.started.elapsed(), self.max_elapsed);
        violation.map_or(Ok(()), |detail| Err(exhausted(detail)))
    }

    pub fn snapshot(&mut self) -> Result<ProductRunProgress, ProductRunnerError> {
        self.check()?;
        Ok(self.state.progress)
    }

    /// Latest successfully recorded counters without performing another fallible host probe.
    #[must_use]
    pub const fn latest_snapshot(&self) -> ProductRunProgress {
        self.state.progress
    }

    pub fn remaining(&self) -> Duration {
        self.max_elapsed.saturating_sub(self.started.elapsed())
    }
}

pub fn validate_run_horizon(max_elapsed: Duration) -> Result<(), ProductRunnerError> {
    if max_elapsed.is_zero() {
        Err(invalid_horizon("configured run horizon must be greater than zero"))
    } else if max_elapsed > PRODUCT_RUN_MAX_ELAPSED {
        Err(invalid_horizon("configured run horizon exceeds the eight-hour hard ceiling"))
    } else {
        Ok(())
    }
}

fn budget_violation(
    progress: ProductRunProgress,
    elapsed: Duration,
    max_elapsed: Duration,
) -> Option<&'static str> {
    progress.budget_violation(elapsed > max_elapsed).map(|violation| match violation {
        BudgetViolation::Elapsed => "the configured run horizon was exhausted",
        BudgetViolation::ModelRequests => "the cumulative provider-request budget was exhausted",
        BudgetViolation::ToolCalls => "the cumulative application-tool budget was exhausted",
        BudgetViolation::TotalTokens => "the cumulative model-token budget was exhausted",
        BudgetViolation::ProviderCost => {
            "the cumulative provider-estimated cost budget was exhausted"
        }
        BudgetViolation::PeakRss => "the product-run peak resident-memory budget was exhausted",
        BudgetViolation::WorkspaceGrowth => "the product-run workspace-growth budget was exhausted",
    })
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn exhausted(detail: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Budget, "account complete coding run", detail)
}

fn invalid_horizon(detail: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidPrecondition,
        "configure coding run horizon",
        detail,
    )
}

#[cfg(test)]
mod tests;
