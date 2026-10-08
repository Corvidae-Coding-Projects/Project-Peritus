//! Independent compiled physical page and statistical-work bounds.

use crate::{EvaluationError, EvaluationErrorKind, EvaluationOperation, invalid};

/// Caller-selected physical work for one resumable bootstrap turn.
///
/// This bounds one invocation, not the cumulative statistical workload frozen in the profile.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BootstrapBatchWork {
    maximum_draws: u64,
}

impl BootstrapBatchWork {
    /// Creates one nonempty physical draw batch.
    ///
    /// # Errors
    /// Rejects zero work. The caller remains responsible for selecting a responsive batch size.
    pub const fn new(maximum_draws: u64) -> Result<Self, EvaluationError> {
        if maximum_draws == 0 {
            Err(invalid(
                EvaluationErrorKind::LimitExceeded,
                EvaluationOperation::Analyze,
                "bootstrap physical batch has zero draws",
            ))
        } else {
            Ok(Self { maximum_draws })
        }
    }

    /// Maximum deterministic draws performed before the next durable boundary.
    #[must_use]
    pub const fn maximum_draws(self) -> u64 {
        self.maximum_draws
    }
}

/// Complete independently enforced physical page and statistical-work limits.
///
/// Task, rollout, attempt, and state bounds size one physical page. They never limit the
/// cumulative logical work performed by a campaign. Bootstrap and pass@k bounds remain explicit
/// caller-selected statistical work limits.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EvaluationLimits {
    tasks: u32,
    rollouts: u32,
    attempts_per_rollout: u16,
    bootstrap_replicates: u32,
    pass_k_values: u16,
    state_bytes: u64,
}

impl EvaluationLimits {
    /// Compiled task descriptors per physical page.
    pub const MAX_TASKS: u32 = 2_048;
    /// Compiled rollout descriptors per physical page.
    pub const MAX_ROLLOUTS: u32 = 16_384;
    /// Compiled attempt records per physical page.
    pub const MAX_ATTEMPTS_PER_ROLLOUT: u16 = 16;
    /// Compiled cumulative bootstrap request ceiling.
    pub const MAX_BOOTSTRAP_REPLICATES: u32 = 100_000;
    /// Compiled distinct pass@k values.
    pub const MAX_PASS_K_VALUES: u16 = 32;
    /// C0 physical state-page ceiling.
    pub const MAX_STATE_BYTES: u64 = 16 * 1024 * 1024;

    /// Creates a complete checked limit set.
    ///
    /// # Errors
    /// Rejects zero or values beyond compiled and C0 ceilings.
    pub const fn new(
        tasks: u32,
        rollouts: u32,
        attempts_per_rollout: u16,
        bootstrap_replicates: u32,
        pass_k_values: u16,
        state_bytes: u64,
    ) -> Result<Self, EvaluationError> {
        if tasks == 0
            || tasks > Self::MAX_TASKS
            || rollouts == 0
            || rollouts > Self::MAX_ROLLOUTS
            || attempts_per_rollout == 0
            || attempts_per_rollout > Self::MAX_ATTEMPTS_PER_ROLLOUT
            || bootstrap_replicates == 0
            || bootstrap_replicates > Self::MAX_BOOTSTRAP_REPLICATES
            || pass_k_values == 0
            || pass_k_values > Self::MAX_PASS_K_VALUES
            || state_bytes == 0
            || state_bytes > Self::MAX_STATE_BYTES
        {
            return Err(invalid(
                EvaluationErrorKind::LimitExceeded,
                EvaluationOperation::FreezeProfile,
                "evaluation limits are zero or exceed compiled/C0 ceilings",
            ));
        }
        Ok(Self {
            tasks,
            rollouts,
            attempts_per_rollout,
            bootstrap_replicates,
            pass_k_values,
            state_bytes,
        })
    }

    /// Production defaults for bounded physical pages and statistical work.
    #[must_use]
    pub const fn production() -> Self {
        Self {
            tasks: Self::MAX_TASKS,
            rollouts: Self::MAX_ROLLOUTS,
            attempts_per_rollout: 8,
            bootstrap_replicates: 10_000,
            pass_k_values: 16,
            state_bytes: Self::MAX_STATE_BYTES,
        }
    }

    /// Maximum task descriptors in one physical page.
    #[must_use]
    pub const fn tasks(self) -> u32 {
        self.tasks
    }
    /// Maximum task descriptors in one physical page.
    #[must_use]
    pub const fn tasks_per_page(self) -> u32 {
        self.tasks
    }
    /// Maximum rollout descriptors in one physical page.
    #[must_use]
    pub const fn rollouts(self) -> u32 {
        self.rollouts
    }
    /// Maximum rollout descriptors in one physical page.
    #[must_use]
    pub const fn rollouts_per_page(self) -> u32 {
        self.rollouts
    }
    /// Maximum attempt records in one physical page.
    #[must_use]
    pub const fn attempts_per_rollout(self) -> u16 {
        self.attempts_per_rollout
    }
    /// Maximum attempt records in one physical page.
    #[must_use]
    pub const fn attempts_per_page(self) -> u16 {
        self.attempts_per_rollout
    }
    /// Maximum bootstrap replicates.
    #[must_use]
    pub const fn bootstrap_replicates(self) -> u32 {
        self.bootstrap_replicates
    }
    /// Maximum requested pass@k values.
    #[must_use]
    pub const fn pass_k_values(self) -> u16 {
        self.pass_k_values
    }
    /// Maximum bytes in one physical state page.
    #[must_use]
    pub const fn state_bytes(self) -> u64 {
        self.state_bytes
    }
    /// Maximum bytes in one physical state page.
    #[must_use]
    pub const fn state_page_bytes(self) -> u64 {
        self.state_bytes
    }
}
