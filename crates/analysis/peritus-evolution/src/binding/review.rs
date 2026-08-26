//! Exact terminal D2 independent-review capture.

use crate::{
    EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery,
    ProductionHarnessBinding, identity::digest_parts,
};
use peritus_review::{ReviewRunPhase, ReviewRunState, ReviewTerminalKind};
use peritus_types::{RunId, Sha256Digest};

/// Completed independent D2 review bound to one exact E1 candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PromotionReviewEvidence {
    run_id: RunId,
    binding_digest: Sha256Digest,
    state_digest: Sha256Digest,
    terminal_digest: Sha256Digest,
    candidate_revision_digest: Sha256Digest,
    tree_digest: Sha256Digest,
    digest: Sha256Digest,
}

impl PromotionReviewEvidence {
    /// Captures a truthful terminal D2 completion for one F0 candidate.
    ///
    /// # Errors
    /// Rejects incomplete quorum, unconserved findings, non-completed terminal state, or candidate
    /// revision/digest drift.
    pub fn capture(
        state: &ReviewRunState,
        candidate: ProductionHarnessBinding,
    ) -> Result<Self, EvolutionError> {
        let terminal = state.terminal().ok_or_else(incomplete)?;
        let candidate_digest = candidate.harness_revision().digest().digest();
        if state.phase() != ReviewRunPhase::Terminal
            || terminal.kind() != ReviewTerminalKind::Completed
            || !state.quorum().complete()
            || !state.unconserved_current_findings().is_empty()
            || !peritus_review::no_implicit_success(state)
            || state.binding().revision() != candidate.revision()
            || state.binding().candidate_digest() != candidate_digest
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::BindingDrift,
                EvolutionOperation::BindReview,
                EvolutionRecovery::ObtainEvidence,
                "review is incomplete or bound to another candidate",
            ));
        }
        let digest = digest_parts(
            b"peritus.f0.promotion-review-evidence.v1\0",
            &[
                state.run_id().as_bytes(),
                state.binding().digest().as_bytes(),
                state.state_digest().as_bytes(),
                terminal.digest().as_bytes(),
                candidate_digest.as_bytes(),
                state.binding().tree_digest().as_bytes(),
            ],
        );
        Ok(Self {
            run_id: state.run_id(),
            binding_digest: state.binding().digest(),
            state_digest: state.state_digest(),
            terminal_digest: terminal.digest(),
            candidate_revision_digest: candidate_digest,
            tree_digest: state.binding().tree_digest(),
            digest,
        })
    }

    #[allow(clippy::too_many_arguments, reason = "all persisted D2 bridge facts stay explicit")]
    pub(crate) fn from_exact_parts(
        run_id: RunId,
        binding_digest: Sha256Digest,
        state_digest: Sha256Digest,
        terminal_digest: Sha256Digest,
        candidate_revision_digest: Sha256Digest,
        tree_digest: Sha256Digest,
    ) -> Self {
        let digest = digest_parts(
            b"peritus.f0.promotion-review-evidence.v1\0",
            &[
                run_id.as_bytes(),
                binding_digest.as_bytes(),
                state_digest.as_bytes(),
                terminal_digest.as_bytes(),
                candidate_revision_digest.as_bytes(),
                tree_digest.as_bytes(),
            ],
        );
        Self {
            run_id,
            binding_digest,
            state_digest,
            terminal_digest,
            candidate_revision_digest,
            tree_digest,
            digest,
        }
    }

    /// Returns the D2 review-run identity.
    #[must_use]
    pub const fn run_id(self) -> RunId {
        self.run_id
    }
    /// Returns the immutable D2 candidate/contract binding digest.
    #[must_use]
    pub const fn binding_digest(self) -> Sha256Digest {
        self.binding_digest
    }
    /// Returns the complete terminal D2 state digest.
    #[must_use]
    pub const fn state_digest(self) -> Sha256Digest {
        self.state_digest
    }
    /// Returns the truthful completed terminal digest.
    #[must_use]
    pub const fn terminal_digest(self) -> Sha256Digest {
        self.terminal_digest
    }
    /// Returns the exact reviewed E1 candidate revision digest.
    #[must_use]
    pub const fn candidate_revision_digest(self) -> Sha256Digest {
        self.candidate_revision_digest
    }
    /// Returns the independently bound candidate tree digest.
    #[must_use]
    pub const fn tree_digest(self) -> Sha256Digest {
        self.tree_digest
    }
    /// Returns the digest of every retained review fact.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
}

const fn incomplete() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::IncompleteEvidence,
        EvolutionOperation::BindReview,
        EvolutionRecovery::ObtainEvidence,
        "review has no terminal completion",
    )
}
