//! Rebuildable query projection for the production-harness pointer.

use peritus_types::{ProjectId, Sha256Digest};

use crate::{
    ActivationId, PointerPhase, ProductionHarnessBinding, ProductionHarnessState, PromotionId,
    RollbackId,
};

/// Compact non-authoritative pointer and activation-history query value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProductionHarnessProjection {
    project_id: ProjectId,
    current: ProductionHarnessBinding,
    phase: PointerPhase,
    sequence: u64,
    generation: u64,
    retained_activations: u32,
    latest_activation: Option<ActivationId>,
    pending_promotion: Option<PromotionId>,
    pending_rollback: Option<RollbackId>,
    state_digest: Sha256Digest,
}

impl ProductionHarnessProjection {
    /// Rebuilds a query value solely from authoritative replayed pointer state.
    #[must_use]
    pub fn from_state(state: &ProductionHarnessState) -> Self {
        let (pending_promotion, pending_rollback) = match state.pending() {
            Some(crate::PendingActivation::Promotion(value)) => (Some(value.id()), None),
            Some(crate::PendingActivation::Rollback(value)) => (None, Some(value.id())),
            None => (None, None),
        };
        Self {
            project_id: state.project_id(),
            current: state.current(),
            phase: state.phase(),
            sequence: state.sequence(),
            generation: state.generation(),
            retained_activations: u32::try_from(state.history().len()).unwrap_or(u32::MAX),
            latest_activation: state.history().last().map(crate::ActivationRecord::id),
            pending_promotion,
            pending_rollback,
            state_digest: state.state_digest(),
        }
    }
    /// Project identity.
    #[must_use]
    pub const fn project_id(self) -> ProjectId {
        self.project_id
    }
    /// Exact current installed E1 binding.
    #[must_use]
    pub const fn current(self) -> ProductionHarnessBinding {
        self.current
    }
    /// Pointer lifecycle phase.
    #[must_use]
    pub const fn phase(self) -> PointerPhase {
        self.phase
    }
    /// Applied pointer event sequence.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }
    /// Monotonic production generation.
    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }
    /// Number of activation records retained for rollback.
    #[must_use]
    pub const fn retained_activations(self) -> u32 {
        self.retained_activations
    }
    /// Latest activation identity.
    #[must_use]
    pub const fn latest_activation(self) -> Option<ActivationId> {
        self.latest_activation
    }
    /// Prepared promotion, if any.
    #[must_use]
    pub const fn pending_promotion(self) -> Option<PromotionId> {
        self.pending_promotion
    }
    /// Prepared rollback, if any.
    #[must_use]
    pub const fn pending_rollback(self) -> Option<RollbackId> {
        self.pending_rollback
    }
    /// Complete authoritative state digest.
    #[must_use]
    pub const fn state_digest(self) -> Sha256Digest {
        self.state_digest
    }
}
