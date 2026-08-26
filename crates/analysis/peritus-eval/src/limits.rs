//! Independent compiled and caller-selected E3 bounds.

use crate::{EvaluationError, EvaluationErrorKind, EvaluationOperation, invalid};

/// Complete independently enforced campaign limits.
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
    /// Compiled task ceiling.
    pub const MAX_TASKS: u32 = 2_048;
    /// Compiled logical rollout ceiling that keeps compact checkpoints bounded.
    pub const MAX_ROLLOUTS: u32 = 16_384;
    /// Compiled attempts retained per logical rollout.
    pub const MAX_ATTEMPTS_PER_ROLLOUT: u16 = 16;
    /// Compiled deterministic bootstrap replicate ceiling.
    pub const MAX_BOOTSTRAP_REPLICATES: u32 = 100_000;
    /// Compiled distinct pass@k values.
    pub const MAX_PASS_K_VALUES: u16 = 32;
    /// C0 canonical frame/checkpoint ceiling.
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

    /// Production defaults sized below the compact C0 checkpoint ceiling.
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

    /// Maximum tasks.
    #[must_use]
    pub const fn tasks(self) -> u32 {
        self.tasks
    }
    /// Maximum logical rollouts.
    #[must_use]
    pub const fn rollouts(self) -> u32 {
        self.rollouts
    }
    /// Maximum retained attempts per rollout.
    #[must_use]
    pub const fn attempts_per_rollout(self) -> u16 {
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
    /// Maximum complete checkpoint bytes.
    #[must_use]
    pub const fn state_bytes(self) -> u64 {
        self.state_bytes
    }
}
