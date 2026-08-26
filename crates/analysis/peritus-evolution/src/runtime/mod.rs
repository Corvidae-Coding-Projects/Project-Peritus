//! Narrow production adapters over F0's pure decisions and existing effect owners.

mod artifact;
mod authority;
mod publication;
mod recovery;

use peritus_journal::{CommittedApprovalUse, CommittedBatch, SqliteJournal};

pub use artifact::{FinalizedEvolutionArtifact, finalize_evolution_artifact};
pub(crate) use authority::approval_use_digest;
pub use authority::{PromotionAuthority, PromotionAuthorityRequest};
pub use publication::{EvolutionPublication, publish_claimed_evolution};
pub use recovery::{EvolutionRecoveryDecision, EvolutionRecoveryObservation, decide_recovery};

use crate::{
    AtomicActivation, CampaignCommand, CampaignTransition, EvolutionError, PointerCommand,
    PointerTransition, commit_atomic_activation, commit_campaign_transition,
    commit_pointer_transition,
};

/// Production F0 facade over one externally owned C0 journal connection.
pub struct EvolutionRuntime<'a> {
    journal: &'a mut SqliteJournal,
}

impl<'a> EvolutionRuntime<'a> {
    /// Borrows the C0 owner for one runtime composition scope.
    #[must_use]
    pub const fn new(journal: &'a mut SqliteJournal) -> Self {
        Self { journal }
    }

    /// Commits one already pure-decided ordinary campaign transition.
    ///
    /// # Errors
    /// Returns the stable F0 failure when C0 rejects or cannot commit the transition.
    pub fn commit_campaign(
        &mut self,
        command: &CampaignCommand,
        transition: &CampaignTransition,
    ) -> Result<CommittedBatch, EvolutionError> {
        commit_campaign_transition(self.journal, command, transition)
    }

    /// Commits one already pure-decided ordinary pointer transition.
    ///
    /// # Errors
    /// Returns the stable F0 failure when C0 rejects or cannot commit the transition.
    pub fn commit_pointer(
        &mut self,
        command: &PointerCommand,
        transition: &PointerTransition,
    ) -> Result<CommittedBatch, EvolutionError> {
        commit_pointer_transition(self.journal, command, transition)
    }

    /// Commits a promotion/rollback and exact approve-once consumption atomically.
    ///
    /// # Errors
    /// Returns the stable F0 failure if any head, state, artifact, registry, or approval fence
    /// rejects the complete transaction.
    pub fn activate(
        &mut self,
        activation: AtomicActivation<'_>,
    ) -> Result<CommittedApprovalUse, EvolutionError> {
        commit_atomic_activation(self.journal, activation)
    }

    /// Borrows the journal for C0 claim, replay, and publication composition.
    #[must_use]
    pub const fn journal(&mut self) -> &mut SqliteJournal {
        self.journal
    }
}
