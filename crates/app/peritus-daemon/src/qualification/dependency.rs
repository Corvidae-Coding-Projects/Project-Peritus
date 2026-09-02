//! Real provider, product-tool, and worker failure qualification over durable scheduler state.

mod effect;
mod scheduler;

use std::path::PathBuf;

use peritus_scheduler::{WorkPhase, WorkTerminal};

use crate::instance::InstanceGuard;
use crate::{DaemonConfig, DaemonError};

use super::{acquire_instance, journal_error, open_journal};
use effect::EffectObservation;
use scheduler::DependencyCampaign;

/// Closed dependency families in the H1 catalog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencyKind {
    /// Executable-backed account provider router.
    Provider,
    /// Grounded product `run_command` tool.
    Tool,
    /// Daemon-owned asynchronous worker task.
    Worker,
}

impl DependencyKind {
    /// Parses one closed command-line value.
    #[must_use]
    pub const fn parse(value: &str) -> Option<Self> {
        match value.as_bytes() {
            b"provider" => Some(Self::Provider),
            b"tool" => Some(Self::Tool),
            b"worker" => Some(Self::Worker),
            _ => None,
        }
    }

    /// Returns the stable catalog code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Provider => "provider",
            Self::Tool => "tool",
            Self::Worker => "worker",
        }
    }
}

/// Closed fault modes shared by the three dependency families.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencyFault {
    /// One owned attempt loses its dependency and is recovered for retry.
    Death,
    /// Real failed attempts consume the supplied immutable retry budget.
    RetryExhaustion,
}

impl DependencyFault {
    /// Parses one closed command-line value.
    #[must_use]
    pub const fn parse(value: &str) -> Option<Self> {
        match value.as_bytes() {
            b"death" => Some(Self::Death),
            b"retry-exhaustion" => Some(Self::RetryExhaustion),
            _ => None,
        }
    }

    /// Returns the stable catalog code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Death => "death",
            Self::RetryExhaustion => "retry-exhaustion",
        }
    }

    const fn attempts(self, retry_limit: u16) -> u16 {
        match self {
            Self::Death => 1,
            Self::RetryExhaustion => retry_limit,
        }
    }

    const fn work_limit(self, retry_limit: u16) -> u16 {
        match self {
            Self::Death => 2,
            Self::RetryExhaustion => retry_limit,
        }
    }
}

/// Direct facts retained after fault injection and durable scheduler settlement.
pub struct DependencyCheckpoint {
    state_sha256: String,
    effect_sha256: String,
    attempts: u16,
    committed_events: u64,
    receipt_bytes: u64,
    child_exit_code: Option<i32>,
    _instance: InstanceGuard,
}

impl DependencyCheckpoint {
    pub(crate) fn state_sha256(&self) -> &str {
        &self.state_sha256
    }
    pub(crate) fn effect_sha256(&self) -> &str {
        &self.effect_sha256
    }
    pub(crate) const fn attempts(&self) -> u16 {
        self.attempts
    }
    pub(crate) const fn committed_events(&self) -> u64 {
        self.committed_events
    }
    pub(crate) const fn receipt_bytes(&self) -> u64 {
        self.receipt_bytes
    }
    pub(crate) const fn child_exit_code(&self) -> Option<i32> {
        self.child_exit_code
    }
}

/// Fresh-process recovery facts derived from replayed scheduler authority.
pub struct DependencyQualification {
    state_sha256: String,
    attempts: u16,
    committed_events: u64,
    aggregate_heads: u64,
    retry_pending: bool,
    exhausted: bool,
    ownership_reconciled: bool,
}

impl DependencyQualification {
    pub(crate) fn state_sha256(&self) -> &str {
        &self.state_sha256
    }
    pub(crate) const fn attempts(&self) -> u16 {
        self.attempts
    }
    pub(crate) const fn committed_events(&self) -> u64 {
        self.committed_events
    }
    pub(crate) const fn aggregate_heads(&self) -> u64 {
        self.aggregate_heads
    }
    pub(crate) const fn retry_pending(&self) -> bool {
        self.retry_pending
    }
    pub(crate) const fn exhausted(&self) -> bool {
        self.exhausted
    }
    pub(crate) const fn ownership_reconciled(&self) -> bool {
        self.ownership_reconciled
    }
    pub(crate) const fn journal_verified(&self) -> bool {
        true
    }
}

/// Executes real dependency failures and durably records their scheduler outcomes.
pub fn stage_dependency_fault(
    config: &DaemonConfig,
    dependency: DependencyKind,
    fault: DependencyFault,
    retry_limit: u16,
) -> Result<DependencyCheckpoint, DaemonError> {
    validate_retry_limit(retry_limit)?;
    let store_id = config.store_identity()?;
    let instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    let executable = std::env::current_exe().map_err(|error| {
        dependency_error("resolve staged candidate executable", error.to_string())
    })?;
    let mut campaign = DependencyCampaign::start(
        &mut journal,
        store_id,
        dependency,
        fault,
        fault.work_limit(retry_limit),
    )?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| dependency_error("construct dependency runtime", error.to_string()))?;
    let mut last_effect = None;
    let attempts = fault.attempts(retry_limit);
    for attempt in 1..=attempts {
        let reservation = campaign.active_reservation()?;
        let observation = runtime.block_on(effect::observe(
            config,
            dependency,
            attempt,
            &reservation,
            &executable,
        ))?;
        campaign.fail_retryable(&mut journal, observation.digest())?;
        last_effect = Some(observation);
        if attempt < attempts {
            campaign.retry_and_dispatch(&mut journal)?;
        }
    }
    let effect = last_effect.ok_or_else(|| {
        dependency_error("inject dependency fault", "no dependency attempt was executed")
    })?;
    let report = journal.integrity_scan().map_err(journal_error)?;
    campaign.verify_stage(fault, attempts)?;
    Ok(checkpoint(campaign, report.event_count(), effect, instance))
}

/// Replays the exact scheduler aggregate and performs any pending safe requeue.
pub fn recover_dependency_fault(
    config: &DaemonConfig,
    dependency: DependencyKind,
    fault: DependencyFault,
    retry_limit: u16,
) -> Result<DependencyQualification, DaemonError> {
    validate_retry_limit(retry_limit)?;
    let store_id = config.store_identity()?;
    let _instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    let mut campaign = DependencyCampaign::reopen(
        &journal,
        store_id,
        dependency,
        fault,
        fault.work_limit(retry_limit),
    )?;
    if fault == DependencyFault::Death {
        campaign.retry_pending(&mut journal)?;
    }
    let report = journal.integrity_scan().map_err(journal_error)?;
    let state = campaign.state();
    let work = state.work().first().ok_or_else(|| {
        dependency_error("recover dependency scheduler", "qualified work item is absent")
    })?;
    let exhausted = matches!(work.terminal(), Some(WorkTerminal::Exhausted { .. }));
    let retry_pending = work.phase() == WorkPhase::RetryPending;
    let ownership_reconciled = state.reservations().is_empty()
        && match fault {
            DependencyFault::Death => work.phase() == WorkPhase::Queued && !exhausted,
            DependencyFault::RetryExhaustion => work.phase() == WorkPhase::Terminal && exhausted,
        };
    if !ownership_reconciled
        || report.event_count() != state.sequence().get()
        || report.aggregate_count() != 1
        || work.attempts_started() != fault.attempts(retry_limit)
    {
        return Err(dependency_error(
            "recover dependency scheduler",
            "replayed work, ownership, retry count, or journal facts differ",
        ));
    }
    Ok(DependencyQualification {
        state_sha256: scheduler::digest_hex(state.state_digest()),
        attempts: work.attempts_started(),
        committed_events: report.event_count(),
        aggregate_heads: report.aggregate_count(),
        retry_pending,
        exhausted,
        ownership_reconciled,
    })
}

fn checkpoint(
    campaign: DependencyCampaign,
    committed_events: u64,
    effect: EffectObservation,
    instance: InstanceGuard,
) -> DependencyCheckpoint {
    DependencyCheckpoint {
        state_sha256: scheduler::digest_hex(campaign.state().state_digest()),
        effect_sha256: scheduler::digest_hex(effect.digest()),
        attempts: campaign.state().work()[0].attempts_started(),
        committed_events,
        receipt_bytes: effect.receipt_bytes(),
        child_exit_code: effect.child_exit_code(),
        _instance: instance,
    }
}

pub(super) fn dependency_error(operation: &'static str, detail: impl Into<String>) -> DaemonError {
    DaemonError::new(
        crate::DaemonErrorCode::Storage,
        crate::DaemonRecovery::Reconcile,
        operation,
        detail,
    )
}

pub(super) fn receipt_path(
    config: &DaemonConfig,
    dependency: DependencyKind,
    attempt: u16,
) -> PathBuf {
    config
        .paths()
        .state_root()
        .join("qualification")
        .join(format!("{}-{attempt}.effects", dependency.code()))
}

fn validate_retry_limit(retry_limit: u16) -> Result<(), DaemonError> {
    if (1..=32).contains(&retry_limit) {
        Ok(())
    } else {
        Err(dependency_error("validate dependency retry limit", "retry limit must be in 1..=32"))
    }
}
