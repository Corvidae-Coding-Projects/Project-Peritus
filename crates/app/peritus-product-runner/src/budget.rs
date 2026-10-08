//! Cumulative accounting and caller-selected elapsed horizon for one complete product run.

use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use peritus_agent::{DeveloperAccountingEvent, DeveloperUsage};

use crate::{ProductRunnerError, ProductRunnerErrorKind, ProductRunnerFailureCause};

mod folder;
#[path = "resource_probe.rs"]
mod resource_probe;

use resource_probe::RunResourceProbe;

pub use crate::accounting::{
    ProductRunProgress, ResourceIoErrorKind, ResourceMeasurement, ResourceMeasurementStatus,
    ResourceObservationCause, ResourceObservationCoverage, ResourceObservationOperation,
};
use crate::accounting::{
    AccountingError, AccountingState, BudgetViolation, UsageSnapshot, WorkEvent,
};

pub struct RunAccounting {
    started: Instant,
    selected_horizon: Option<Duration>,
    state: AccountingState,
    resources: RunResourceProbe,
    pending_failure: Option<ProductRunnerError>,
}

impl RunAccounting {
    pub fn new(
        workspace_root: &Path,
        max_elapsed: Option<Duration>,
    ) -> Result<Self, ProductRunnerError> {
        Self::new_with_cancellation(
            workspace_root,
            max_elapsed,
            Arc::new(AtomicBool::new(false)),
        )
    }

    pub(crate) fn new_with_cancellation(
        workspace_root: &Path,
        max_elapsed: Option<Duration>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self, ProductRunnerError> {
        validate_run_horizon(max_elapsed)?;
        Ok(Self {
            started: Instant::now(),
            selected_horizon: max_elapsed,
            state: AccountingState::default(),
            resources: RunResourceProbe::new_with_cancellation(workspace_root, cancelled),
            pending_failure: None,
        })
    }

    pub(crate) fn record_event(
        &mut self,
        event: DeveloperAccountingEvent,
    ) -> Result<(), ProductRunnerError> {
        if let Some(error) = self.pending_failure.as_ref() {
            return Err(error.clone());
        }
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
        budget_violation(self.started.elapsed(), self.selected_horizon)
            .map_or(Ok(()), |detail| Err(deadline_exhausted(detail)))
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

    pub fn check(&mut self) -> Result<(), ProductRunnerError> {
        if let Some(error) = self.pending_failure.take() {
            return Err(error);
        }
        self.state.progress.elapsed_millis = millis(self.started.elapsed());
        // Observation availability is data, not an implicit run deadline. The probe publishes a
        // typed unavailable/partial measurement and remains attached to this accounting context.
        let resources = self.resources.observe();
        self.state.progress.workspace_measurement = resources.workspace_measurement;
        self.state.progress.workspace_growth_measurement = resources.growth_measurement;
        self.state.progress.peak_rss_measurement = resources.peak_rss_measurement;
        if resources.workspace_measurement.measured_value().is_some() {
            self.state.progress.workspace_bytes = resources.workspace;
        }
        if resources.growth_measurement.measured_value().is_some() {
            self.state.progress.workspace_growth_bytes =
                self.state.progress.workspace_growth_bytes.max(resources.growth);
        }
        if resources.peak_rss_measurement.value().is_some() {
            self.state.progress.peak_rss_bytes =
                self.state.progress.peak_rss_bytes.max(resources.peak_rss);
        }
        let violation = budget_violation(self.started.elapsed(), self.selected_horizon);
        violation.map_or(Ok(()), |detail| Err(deadline_exhausted(detail)))
    }

    /// Preserve the first host accounting rejection across the developer trace adapter.
    pub(crate) fn retain_failure(&mut self, error: ProductRunnerError) {
        if self.pending_failure.is_none() {
            self.pending_failure = Some(error);
        }
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

    pub fn remaining(&self) -> Option<Duration> {
        self.selected_horizon.map(|limit| limit.saturating_sub(self.started.elapsed()))
    }
}

pub fn validate_run_horizon(max_elapsed: Option<Duration>) -> Result<(), ProductRunnerError> {
    let Some(max_elapsed) = max_elapsed else { return Ok(()) };
    if max_elapsed.is_zero() {
        Err(invalid_horizon("configured run horizon must be greater than zero"))
    } else {
        Ok(())
    }
}

fn budget_violation(elapsed: Duration, max_elapsed: Option<Duration>) -> Option<&'static str> {
    BudgetViolation::from_elapsed(max_elapsed.is_some_and(|limit| elapsed > limit)).map(
        |violation| match violation {
            BudgetViolation::Elapsed => "the configured run horizon was exhausted",
        },
    )
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn exhausted(detail: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Budget, "account complete coding run", detail)
        .with_failure_cause(ProductRunnerFailureCause::AccountingRepresentation)
}

fn deadline_exhausted(detail: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Budget, "account complete coding run", detail)
        .with_failure_cause(ProductRunnerFailureCause::SelectedDeadline)
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
