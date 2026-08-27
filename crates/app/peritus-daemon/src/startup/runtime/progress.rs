//! Live binding between startup effects, the canonical checkpoint plan, and public lifecycle.

use peritus_journal::StoreId;
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};

use crate::{DaemonError, DaemonErrorCode, DaemonLifecycle, DaemonRecovery, StartupPhase};

use super::super::plan::{StartupFailpoints, StartupPlan};

/// Sole live owner of the canonical startup plan.
pub(super) struct StartupProgress {
    store_id: StoreId,
    plan: StartupPlan,
    lifecycle: DaemonLifecycle,
}

impl StartupProgress {
    pub(super) const fn new(store_id: StoreId) -> Self {
        Self {
            store_id,
            plan: StartupPlan::new(StartupFailpoints::none()),
            lifecycle: DaemonLifecycle::starting(),
        }
    }

    /// Completes one live effect boundary and retains its canonical checkpoint.
    pub(super) fn complete(&mut self, phase: StartupPhase) -> Result<(), DaemonError> {
        let selected = self.plan.begin_next().map_err(plan_error)?;
        if selected != Some(phase) {
            return Err(DaemonError::new(
                DaemonErrorCode::CorruptState,
                DaemonRecovery::Operator,
                "execute daemon startup plan",
                "live startup effect differs from the canonical next phase",
            ));
        }
        if phase != StartupPhase::Validate {
            self.lifecycle.advance(phase)?;
        }
        let ordinal = self.plan.checkpoints().len();
        self.plan
            .complete_active(
                phase,
                checkpoint_digest(self.store_id, phase, ordinal, b"input"),
                checkpoint_digest(self.store_id, phase, ordinal, b"output"),
            )
            .map(|_| ())
            .map_err(plan_error)
    }

    pub(super) fn into_lifecycle(self) -> Result<DaemonLifecycle, DaemonError> {
        if !self.plan.checkpoints().is_complete() {
            return Err(DaemonError::new(
                DaemonErrorCode::CorruptState,
                DaemonRecovery::Operator,
                "complete daemon startup plan",
                "startup reached runtime construction without every canonical checkpoint",
            ));
        }
        Ok(self.lifecycle)
    }
}

fn checkpoint_digest(
    store_id: StoreId,
    phase: StartupPhase,
    ordinal: u8,
    boundary: &[u8],
) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/daemon-startup-checkpoint/v1\0");
    hasher.update(store_id.as_bytes());
    hasher.update([phase_tag(phase), ordinal]);
    hasher.update(boundary);
    Sha256Digest::new(hasher.finalize().into())
}

const fn phase_tag(phase: StartupPhase) -> u8 {
    match phase {
        StartupPhase::Validate => 1,
        StartupPhase::Lock => 2,
        StartupPhase::Migrate => 3,
        StartupPhase::Journal => 4,
        StartupPhase::Artifacts => 5,
        StartupPhase::Evidence => 6,
        StartupPhase::Projections => 7,
        StartupPhase::AuthorityEpoch => 8,
        StartupPhase::DomainRecovery => 9,
        StartupPhase::EffectRecovery => 10,
        StartupPhase::AppRecovery => 11,
        StartupPhase::Outbox => 12,
        StartupPhase::Ipc => 13,
        StartupPhase::Ready => 14,
    }
}

fn plan_error(error: impl std::error::Error + Send + Sync + 'static) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        "execute daemon startup plan",
        "canonical startup checkpoint execution failed",
        error,
    )
}
