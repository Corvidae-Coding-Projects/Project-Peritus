//! Explicit approve-once production activation and independently authorized rollback.

use peritus_approval::ApprovalUseOutcome;
use peritus_evolution::{
    ActivationId, AtomicActivation, CampaignCommandKind, CompatibilityWitness, EvolutionCampaignId,
    EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery, PendingActivation,
    PointerCommandKind, ProductionHarnessState, PromotionAuthority, PromotionAuthorityRequest,
    RollbackProposal, decide_campaign, decide_pointer,
};
use peritus_journal::{
    CommittedCapabilityUse, CommittedKernelTransition, CurrentCredentialRegistry,
};
use peritus_types::{ActionId, ProjectId, RevisionTuple, Sha256Digest};

use super::{
    EvolutionOperator, EvolutionTransitionIds, campaign_command, incomplete, owner_error,
    pointer_command,
};

/// Existing exact durable B0/B1 observations and a move-only approve-once use decision.
///
/// Construction is inert. Capture checks the actual local journal and the existing authority
/// reducer before the atomic commit consumes the matching approved action.
pub struct EvolutionActivationInput<'a> {
    action: ActionId,
    kernel: &'a CommittedKernelTransition,
    capability: &'a CommittedCapabilityUse,
    approval: ApprovalUseOutcome,
    expected_approval_revision: u64,
    registry: &'a CurrentCredentialRegistry,
}

impl<'a> EvolutionActivationInput<'a> {
    /// Retains the exact owner observations; this does not grant, resolve, or consume approval.
    #[must_use]
    pub const fn new(
        action: ActionId,
        kernel: &'a CommittedKernelTransition,
        capability: &'a CommittedCapabilityUse,
        approval: ApprovalUseOutcome,
        expected_approval_revision: u64,
        registry: &'a CurrentCredentialRegistry,
    ) -> Self {
        Self { action, kernel, capability, approval, expected_approval_revision, registry }
    }
}

impl EvolutionOperator<'_> {
    /// Atomically activates the exact prepared promotion and consumes its existing approve-once use.
    ///
    /// # Errors
    /// Rejects absent, stale, cross-store, mismatched, denied, or reused production authority.
    pub fn activate_promotion(
        &mut self,
        campaign: EvolutionCampaignId,
        input: EvolutionActivationInput<'_>,
        ids: EvolutionTransitionIds,
    ) -> Result<ProductionHarnessState, EvolutionError> {
        let state = self.campaign(campaign)?;
        let proposal =
            state.proposal().ok_or_else(|| incomplete("campaign has no prepared promotion"))?;
        let pointer = self.pointer(state.project_id())?;
        if !matches!(pointer.pending(), Some(PendingActivation::Promotion(value)) if value == proposal)
        {
            return Err(incomplete("pointer does not contain this exact prepared promotion"));
        }
        self.bound_revision(proposal.candidate())?;
        if let Some(review) = proposal.review()
            && self.review(review.run_id(), proposal.candidate())? != review
        {
            return Err(incomplete("promotion independent review no longer matches owning state"));
        }
        let authorization = self
            .activation_authority(
                &input,
                proposal.digest(),
                proposal.project_id(),
                proposal.candidate().revision(),
            )?
            .authorization();
        let campaign_command = campaign_command(
            Some(&state),
            campaign,
            state.policy().policy().digest(),
            ids,
            CampaignCommandKind::ActivatePromotion { activation_digest: authorization.digest() },
        )?;
        let campaign_transition = decide_campaign(Some(&state), &campaign_command)?;
        // Both aggregate changes share one command identity and one C0 atomic transaction.
        let pointer_ids = EvolutionTransitionIds::new(
            ids.command,
            ids.child(b"activation-pointer-event", proposal.digest())?.event,
        );
        let pointer_command = pointer_command(
            Some(&pointer),
            state.project_id(),
            pointer.policy().digest(),
            pointer_ids,
            PointerCommandKind::ActivatePromotion {
                promotion_id: proposal.id(),
                campaign_terminal_digest: campaign_transition.state().state_digest(),
                authorization,
            },
        )?;
        let pointer_transition = decide_pointer(Some(&pointer), &pointer_command)?;
        let activation = AtomicActivation::promotion(
            &campaign_command,
            &campaign_transition,
            &pointer_command,
            &pointer_transition,
            input.approval,
            input.expected_approval_revision,
            input.registry,
        )?;
        peritus_evolution::commit_atomic_activation(self.journal, activation)?;
        self.pointer(state.project_id())
    }

    /// Prepares a retained-target rollback using actual E1 schema compatibility observations.
    ///
    /// The proposal is inert and requires a new approve-once action before activation. This adapter
    /// conservatively rejects schema, provider/platform feature, lineage, or protected-policy changes.
    ///
    /// # Errors
    /// Rejects unknown targets, incompatible schemas, missing receipts, or another pending action.
    pub fn prepare_rollback(
        &mut self,
        project: ProjectId,
        target_activation: ActivationId,
        ids: EvolutionTransitionIds,
    ) -> Result<RollbackProposal, EvolutionError> {
        let pointer = self.pointer(project)?;
        let target = pointer
            .history()
            .iter()
            .find(|value| value.id() == target_activation)
            .ok_or_else(|| incomplete("rollback target activation is not retained"))?
            .successor();
        if let Some(PendingActivation::Rollback(proposal)) = pointer.pending()
            && proposal.target_activation() == target_activation
        {
            return Ok(proposal.clone());
        }
        if pointer.pending().is_some() {
            return Err(incomplete("pointer already has a pending action"));
        }
        let current_revision = self.bound_revision(pointer.current())?;
        let target_revision = self.bound_revision(target)?;
        let declarations = current_revision.graph().declarations();
        let target_declarations = target_revision.graph().declarations();
        let compatible = current_revision.harness_id() == target_revision.harness_id()
            && pointer.policy().preserved_by(&target_revision)
            && declarations.len() == target_declarations.len()
            && declarations.iter().zip(target_declarations).all(|(current, target)| {
                current.id() == target.id()
                    && current.kind() == target.kind()
                    && current.schema_version() == target.schema_version()
                    && current.compatibility() == target.compatibility()
                    && current.declared_authority() == target.declared_authority()
            });
        if !compatible {
            return Err(incomplete(
                "retained target requires an explicit schema or authority migration",
            ));
        }
        let mut bytes = b"PERITUS-F0-ROLLBACK-COMPATIBILITY\0".to_vec();
        bytes.extend_from_slice(pointer.state_digest().as_bytes());
        bytes.extend_from_slice(target_activation.as_bytes());
        bytes.extend_from_slice(&current_revision.canonical_bytes());
        bytes.extend_from_slice(&target_revision.canonical_bytes());
        let semantic = peritus_codec::sha256(&bytes);
        let bundle = peritus_evolution::finalize_evolution_artifact(
            self.artifacts,
            &bytes,
            semantic,
            ids.event,
        )?;
        let witness = CompatibilityWitness::new(
            pointer.current(),
            target,
            pointer.policy().digest(),
            semantic,
            true,
        );
        let proposal = RollbackProposal::new(
            &pointer,
            target,
            target_activation,
            witness,
            bundle.artifact_digest().sha256(),
        )?;
        let command = pointer_command(
            Some(&pointer),
            project,
            pointer.policy().digest(),
            ids,
            PointerCommandKind::PrepareRollback(proposal.clone()),
        )?;
        self.commit_pointer(&command)?;
        Ok(proposal)
    }

    /// Activates a prepared rollback with a new exact approved action.
    ///
    /// # Errors
    /// Rejects missing preparation, stale/fabricated authority, or reused approval.
    pub fn activate_rollback(
        &mut self,
        project: ProjectId,
        input: EvolutionActivationInput<'_>,
        ids: EvolutionTransitionIds,
    ) -> Result<ProductionHarnessState, EvolutionError> {
        let pointer = self.pointer(project)?;
        let Some(PendingActivation::Rollback(proposal)) = pointer.pending() else {
            return Err(incomplete("production pointer has no prepared rollback"));
        };
        self.bound_revision(proposal.target())?;
        let authorization = self
            .activation_authority(&input, proposal.digest(), project, proposal.target().revision())?
            .authorization();
        let command = pointer_command(
            Some(&pointer),
            project,
            pointer.policy().digest(),
            ids,
            PointerCommandKind::ActivateRollback { rollback_id: proposal.id(), authorization },
        )?;
        let transition = decide_pointer(Some(&pointer), &command)?;
        let activation = AtomicActivation::rollback(
            &command,
            &transition,
            input.approval,
            input.expected_approval_revision,
            input.registry,
        )?;
        peritus_evolution::commit_atomic_activation(self.journal, activation)?;
        self.pointer(project)
    }

    fn activation_authority(
        &self,
        input: &EvolutionActivationInput<'_>,
        action_digest: Sha256Digest,
        project: ProjectId,
        revision: RevisionTuple,
    ) -> Result<PromotionAuthority, EvolutionError> {
        if input.kernel.aggregate().project_id() != project
            || input.kernel.aggregate().revision() != revision
            || input.capability.transition().scope().revision() != revision
            || input.approval.transition().revision() != revision
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::Authority,
                EvolutionOperation::Authorize,
                EvolutionRecovery::RequestAuthority,
                "activation authority does not bind the exact project and target revision",
            ));
        }
        self.require_local_batch(input.kernel.batch())?;
        self.require_local_batch(input.capability.batch())?;
        let actual_registry = self.journal.current_credential_registry().map_err(owner_error)?;
        if actual_registry != *input.registry {
            return Err(incomplete(
                "activation credential registry is not the current durable owner state",
            ));
        }
        PromotionAuthority::capture(PromotionAuthorityRequest::new(
            input.action,
            action_digest,
            input.kernel,
            input.capability,
            &input.approval,
        ))
    }
}
