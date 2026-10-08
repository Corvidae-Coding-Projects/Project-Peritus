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
    AtomicActivation, CampaignCommand, CampaignState, CampaignTransition, EvolutionError,
    EvolutionStorageLimits, PointerCommand, PointerTransition, ProductionHarnessState,
    commit_atomic_activation_with_storage,
    commit_campaign_transition_with_storage, commit_pointer_transition_with_storage,
    decide_campaign, decide_pointer, resolve_campaign_receipt, resolve_pointer_receipt,
};

/// Production F0 facade over one externally owned C0 journal connection.
pub struct EvolutionRuntime<'a> {
    journal: &'a mut SqliteJournal,
    storage: EvolutionStorageLimits,
}

impl<'a> EvolutionRuntime<'a> {
    /// Borrows the C0 owner for one runtime composition scope.
    #[must_use]
    pub const fn new(journal: &'a mut SqliteJournal) -> Self {
        Self { journal, storage: EvolutionStorageLimits::journal_default() }
    }

    /// Creates a runtime with explicit physical checkpoint page capacity.
    #[must_use]
    pub const fn with_storage(
        journal: &'a mut SqliteJournal,
        storage: EvolutionStorageLimits,
    ) -> Self {
        Self { journal, storage }
    }

    /// Resolves an already-committed exact campaign command before re-running its reducer.
    ///
    /// # Errors
    /// Rejects a conflicting command identity or corrupt receipt.
    pub fn campaign_receipt(
        &mut self,
        command: &CampaignCommand,
    ) -> Result<Option<CommittedBatch>, EvolutionError> {
        resolve_campaign_receipt(&*self.journal, command)
    }

    /// Resolves an already-committed exact pointer command before re-running its reducer.
    ///
    /// # Errors
    /// Rejects a conflicting command identity or corrupt receipt.
    pub fn pointer_receipt(
        &mut self,
        command: &PointerCommand,
    ) -> Result<Option<CommittedBatch>, EvolutionError> {
        resolve_pointer_receipt(&*self.journal, command)
    }

    /// Resolves an exact receipt before deciding and committing a new campaign command.
    ///
    /// # Errors
    /// Returns the stable F0 failure when the command conflicts, reduction rejects, or C0 cannot
    /// commit the accepted transition.
    pub fn execute_campaign(
        &mut self,
        prior: Option<&CampaignState>,
        command: &CampaignCommand,
    ) -> Result<CommittedBatch, EvolutionError> {
        if let Some(receipt) = resolve_campaign_receipt(&*self.journal, command)? {
            return Ok(receipt);
        }
        let transition = decide_campaign(prior, command)?;
        commit_campaign_transition_with_storage(
            self.journal,
            command,
            &transition,
            self.storage,
        )
    }

    /// Resolves an exact receipt before deciding and committing a new pointer command.
    ///
    /// # Errors
    /// Returns the stable F0 failure when the command conflicts, reduction rejects, or C0 cannot
    /// commit the accepted transition.
    pub fn execute_pointer(
        &mut self,
        prior: Option<&ProductionHarnessState>,
        command: &PointerCommand,
    ) -> Result<CommittedBatch, EvolutionError> {
        if let Some(receipt) = resolve_pointer_receipt(&*self.journal, command)? {
            return Ok(receipt);
        }
        let transition = decide_pointer(prior, command)?;
        commit_pointer_transition_with_storage(
            self.journal,
            command,
            &transition,
            self.storage,
        )
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
        commit_campaign_transition_with_storage(self.journal, command, transition, self.storage)
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
        commit_pointer_transition_with_storage(self.journal, command, transition, self.storage)
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
        commit_atomic_activation_with_storage(self.journal, activation, self.storage)
    }

    /// Borrows the journal for C0 claim, replay, and publication composition.
    #[must_use]
    pub const fn journal(&mut self) -> &mut SqliteJournal {
        self.journal
    }
}
