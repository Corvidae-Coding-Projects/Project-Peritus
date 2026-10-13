//! Production F0 orchestration over the existing durable owners.
//!
//! The serialized daemon authority supplies the stores. No method constructs a materialization,
//! published evaluation, independent review, or promotion authorization from asserted receipts.

mod activation;
mod debugger_publication;
mod evidence;
mod local_diagnosis;
mod local_failure_trace;
mod local_trace;
mod publication;
mod selection;

use peritus_artifact_store::ArtifactStore;
use peritus_evidence::EvidenceStore;
use peritus_evolution::{
    CampaignCommand, CampaignCommandKind, CampaignState, EvolutionCampaignId, EvolutionError,
    EvolutionErrorKind, EvolutionLimits, EvolutionOperation, EvolutionRecovery, PointerCommand,
    PointerCommandKind, ProductionHarnessState, PromotionPolicy, PromotionPolicyBinding,
    decide_campaign, decide_pointer, recover_campaign, recover_pointer,
};
use peritus_harness::{domain::ComponentId, materialization::MaterializationReceiptId};
use peritus_journal::{CommittedBatch, SqliteJournal};
use peritus_types::{CommandId, EventId, EvidenceId, ProjectId, RevisionTuple, Sha256Digest};

pub use activation::EvolutionActivationInput;
pub use debugger_publication::{publish_debugger_claim, stage_local_debugger_report};
pub(crate) use evidence::{resolve_current_binding, resolve_materialized_arm};
pub use local_diagnosis::{NativeDiagnosticReport, local_diagnostic_report};
pub use local_trace::{LocalTraceReceipt, import_local_trace};
pub use publication::publish_evolution_claim;

/// Caller-selected identities for one durable operator transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvolutionTransitionIds {
    command: CommandId,
    event: EventId,
}

impl EvolutionTransitionIds {
    /// Retains the exact operator command and event identities.
    #[must_use]
    pub const fn new(command: CommandId, event: EventId) -> Self {
        Self { command, event }
    }

    pub(super) fn child(self, label: &[u8], digest: Sha256Digest) -> Result<Self, EvolutionError> {
        let mut bytes = b"PERITUS-F0-OPERATOR-CHILD\0".to_vec();
        bytes.extend_from_slice(self.command.as_bytes());
        bytes.extend_from_slice(label);
        bytes.extend_from_slice(digest.as_bytes());
        let command = CommandId::new(identity(&bytes)).map_err(owner_error)?;
        bytes.extend_from_slice(self.event.as_bytes());
        let event = EventId::new(identity(&bytes)).map_err(owner_error)?;
        Ok(Self { command, event })
    }
}

/// Reference to an actual retained E1 materialization in this daemon's journal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaterializedHarnessRef {
    revision: RevisionTuple,
    receipt: MaterializationReceiptId,
}

impl MaterializedHarnessRef {
    /// Identifies a retained receipt; construction grants no authority.
    #[must_use]
    pub const fn new(revision: RevisionTuple, receipt: MaterializationReceiptId) -> Self {
        Self { revision, receipt }
    }
}

/// Narrow operator facade borrowing the daemon's serialized durable owners.
pub struct EvolutionOperator<'a> {
    journal: &'a mut SqliteJournal,
    artifacts: &'a ArtifactStore,
    evidence: &'a mut EvidenceStore,
}

impl<'a> EvolutionOperator<'a> {
    /// Borrows the production owners for one operator request.
    #[must_use]
    pub const fn new(
        journal: &'a mut SqliteJournal,
        artifacts: &'a ArtifactStore,
        evidence: &'a mut EvidenceStore,
    ) -> Self {
        Self { journal, artifacts, evidence }
    }

    /// Establishes a pointer from an actual E1 receipt and publishes its initialization evidence.
    ///
    /// E1 journal events use the harness revision domain. The pointer initialization event creates
    /// the full cross-slice revision and artifact reference required by the evidence owner. Exact
    /// retries reconcile that committed event and the same reserved evidence identity.
    ///
    /// # Errors
    /// Rejects absent or drifted materialization/evidence, policy mismatch, or an existing pointer.
    #[allow(
        clippy::too_many_arguments,
        reason = "initial production authority inputs stay explicit"
    )]
    pub fn initialize(
        &mut self,
        project: ProjectId,
        materialization: MaterializedHarnessRef,
        policy_component: &ComponentId,
        policy: PromotionPolicy,
        limits: EvolutionLimits,
        evidence_id: EvidenceId,
        ids: EvolutionTransitionIds,
    ) -> Result<ProductionHarnessState, EvolutionError> {
        let (revision, binding) = self.materialized(materialization)?;
        let policy = PromotionPolicyBinding::capture(&revision, policy_component, policy)?;
        let artifact = self.materialization_artifact(materialization)?;
        self.artifacts.verify(artifact).map_err(owner_error)?;
        let harness = peritus_harness::load_harness_replay(
            self.journal,
            materialization.revision.harness_id(),
        )
        .map_err(owner_error)?
        .rebuild()
        .map_err(owner_error)?
        .ok_or_else(|| incomplete("initial materialization lineage is absent"))?;
        let receipt = harness
            .receipt(materialization.receipt)
            .ok_or_else(|| incomplete("initial materialization receipt is absent"))?;
        let command = pointer_command(
            None,
            project,
            policy.digest(),
            ids,
            PointerCommandKind::InitializeProductionHarness {
                initial: binding,
                policy,
                limits,
                evidence_artifact: artifact.sha256(),
                evidence_digest: receipt.digest(),
            },
        )?;
        let transition = decide_pointer(None, &command)?;
        let batch =
            peritus_evolution::commit_pointer_transition(self.journal, &command, &transition)?;
        let draft = peritus_evidence::EvidenceDraft::new(
            evidence_id,
            peritus_evidence::EvidenceKind::new("production-harness-materialization")
                .map_err(owner_error)?,
            peritus_evidence::EvidenceSource::new("peritus-evolution").map_err(owner_error)?,
            binding.revision(),
            batch.last_position(),
            receipt.digest(),
            vec![artifact],
            vec![],
        )
        .map_err(owner_error)?;
        if self.evidence.committed_retry(&draft).map_err(owner_error)?.is_none() {
            self.evidence
                .admit(
                    draft,
                    &self.journal.integrity_export().map_err(owner_error)?,
                    self.artifacts,
                )
                .map_err(owner_error)?;
        }
        self.pointer(project)
    }

    /// Creates a campaign from the exact current durable production pointer.
    ///
    /// # Errors
    /// Rejects absent/pending production state, illegal genesis, or stale journal fences.
    pub fn create_campaign(
        &mut self,
        campaign: EvolutionCampaignId,
        project: ProjectId,
        limits: EvolutionLimits,
        ids: EvolutionTransitionIds,
    ) -> Result<CampaignState, EvolutionError> {
        let pointer = self.pointer(project)?;
        if pointer.phase() != peritus_evolution::PointerPhase::Active {
            return Err(incomplete("production pointer has a pending activation"));
        }
        let replay = recover_campaign(self.journal, campaign)?;
        let command = campaign_command(
            replay.state(),
            campaign,
            pointer.policy().policy().digest(),
            ids,
            CampaignCommandKind::CreateCampaign {
                project_id: project,
                baseline: pointer.current(),
                policy: pointer.policy().clone(),
                limits,
            },
        )?;
        self.commit_campaign(&command)
    }

    /// Freezes already assembled immutable campaign inputs.
    ///
    /// # Errors
    /// Rejects missing state or a phase other than Draft.
    pub fn freeze_campaign(
        &mut self,
        campaign: EvolutionCampaignId,
        ids: EvolutionTransitionIds,
    ) -> Result<CampaignState, EvolutionError> {
        self.apply_campaign(campaign, ids, CampaignCommandKind::FreezeCampaign)
    }

    /// Admits actual baseline evidence from C0 and the immutable evidence owner.
    ///
    /// # Errors
    /// Rejects missing, invalidated, wrong-revision, or artifact-free evidence.
    pub fn record_baseline(
        &mut self,
        campaign: EvolutionCampaignId,
        evidence_id: EvidenceId,
        artifact: peritus_artifact_store::ArtifactDigest,
        ids: EvolutionTransitionIds,
    ) -> Result<CampaignState, EvolutionError> {
        let state = self.campaign(campaign)?;
        let evidence = self.current_evidence(evidence_id, state.baseline().revision())?;
        if !evidence.artifacts().contains(&artifact) {
            return Err(incomplete("baseline artifact was not admitted by its evidence owner"));
        }
        self.artifacts.verify(artifact).map_err(owner_error)?;
        self.apply_campaign(
            campaign,
            ids,
            CampaignCommandKind::RecordBaselineEvidence {
                artifact_digest: artifact.sha256(),
                evidence_digest: evidence.record_digest(),
            },
        )
    }

    /// Records an actual published decision receipt after the campaign reaches a terminal state.
    ///
    /// # Errors
    /// Rejects early acknowledgement, missing publication evidence, or any action/artifact drift.
    pub fn record_decision_publication(
        &mut self,
        campaign: EvolutionCampaignId,
        evidence_id: EvidenceId,
        ids: EvolutionTransitionIds,
    ) -> Result<CampaignState, EvolutionError> {
        let state = self.campaign(campaign)?;
        let proposal = state
            .proposal()
            .ok_or_else(|| incomplete("campaign has no decision proposal to publish"))?;
        if state.terminal().is_none() {
            return Err(incomplete(
                "campaign decision acknowledgement requires a terminal campaign",
            ));
        }
        let evidence = self.current_evidence(evidence_id, proposal.candidate().revision())?;
        let artifact = peritus_artifact_store::ArtifactDigest::from_sha256(
            proposal.evidence_bundle_artifact(),
        );
        if evidence.kind().as_str() != "evolution-decision"
            || evidence.source().as_str() != "peritus-evolution"
            || evidence.payload_digest() != proposal.digest()
            || !evidence.artifacts().contains(&artifact)
        {
            return Err(incomplete(
                "decision publication evidence belongs to another exact action",
            ));
        }
        self.artifacts.verify(artifact).map_err(owner_error)?;
        let publication = peritus_evolution::CampaignPublication::new(
            artifact.sha256(),
            evidence_id,
            evidence.record_digest(),
            evidence.provenance().global_position(),
        )?;
        if let Some(existing) = state.publication() {
            if existing == publication {
                return Ok(state);
            }
            return Err(incomplete("campaign already acknowledges another publication"));
        }
        self.apply_campaign(campaign, ids, CampaignCommandKind::RecordPublication(publication))
    }

    /// Returns reducer-replayed current campaign state.
    ///
    /// # Errors
    /// Rejects absent or contradictory durable state.
    pub fn campaign(&self, campaign: EvolutionCampaignId) -> Result<CampaignState, EvolutionError> {
        recover_campaign(self.journal, campaign)?
            .state()
            .cloned()
            .ok_or_else(|| incomplete("evolution campaign does not exist"))
    }

    /// Returns reducer-replayed current production state.
    ///
    /// # Errors
    /// Rejects absent or contradictory durable state.
    pub fn pointer(&self, project: ProjectId) -> Result<ProductionHarnessState, EvolutionError> {
        recover_pointer(self.journal, project)?
            .state()
            .cloned()
            .ok_or_else(|| incomplete("production harness pointer does not exist"))
    }

    pub(super) fn apply_campaign(
        &mut self,
        campaign: EvolutionCampaignId,
        ids: EvolutionTransitionIds,
        kind: CampaignCommandKind,
    ) -> Result<CampaignState, EvolutionError> {
        let state = self.campaign(campaign)?;
        let command =
            campaign_command(Some(&state), campaign, state.policy().policy().digest(), ids, kind)?;
        self.commit_campaign(&command)
    }

    pub(super) fn commit_campaign(
        &mut self,
        command: &CampaignCommand,
    ) -> Result<CampaignState, EvolutionError> {
        let replay = recover_campaign(self.journal, command.campaign_id())?;
        let transition = decide_campaign(replay.state(), command)?;
        peritus_evolution::commit_campaign_transition(self.journal, command, &transition)?;
        self.campaign(command.campaign_id())
    }

    pub(super) fn commit_pointer(
        &mut self,
        command: &PointerCommand,
    ) -> Result<ProductionHarnessState, EvolutionError> {
        let replay = recover_pointer(self.journal, command.project_id())?;
        let transition = decide_pointer(replay.state(), command)?;
        peritus_evolution::commit_pointer_transition(self.journal, command, &transition)?;
        self.pointer(command.project_id())
    }

    pub(super) fn require_local_batch(&self, batch: &CommittedBatch) -> Result<(), EvolutionError> {
        match self
            .journal
            .resolve_command(batch.command_id(), batch.request_digest())
            .map_err(owner_error)?
        {
            peritus_journal::CommandResolution::Committed(actual) if actual == *batch => Ok(()),
            _ => Err(incomplete("authority observation is absent from this daemon's journal")),
        }
    }
}

pub(super) fn campaign_command(
    prior: Option<&CampaignState>,
    campaign: EvolutionCampaignId,
    policy: Sha256Digest,
    ids: EvolutionTransitionIds,
    kind: CampaignCommandKind,
) -> Result<CampaignCommand, EvolutionError> {
    CampaignCommand::new(
        ids.command,
        ids.event,
        campaign,
        prior.map_or(0, CampaignState::sequence),
        prior.map(CampaignState::last_event),
        prior.map_or(Sha256Digest::new([0; 32]), CampaignState::state_digest),
        policy,
        kind,
    )
}

pub(super) fn pointer_command(
    prior: Option<&ProductionHarnessState>,
    project: ProjectId,
    policy: Sha256Digest,
    ids: EvolutionTransitionIds,
    kind: PointerCommandKind,
) -> Result<PointerCommand, EvolutionError> {
    PointerCommand::new(
        ids.command,
        ids.event,
        project,
        prior.map_or(0, ProductionHarnessState::sequence),
        prior.map(ProductionHarnessState::last_event),
        prior.map_or(0, ProductionHarnessState::generation),
        prior.map_or(Sha256Digest::new([0; 32]), ProductionHarnessState::state_digest),
        policy,
        kind,
    )
}

pub(super) fn identity(bytes: &[u8]) -> [u8; 16] {
    let digest = peritus_codec::sha256(bytes);
    let mut id = [0_u8; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    if id == [0; 16] {
        id[15] = 1;
    }
    id
}

pub(super) const fn incomplete(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::IncompleteEvidence,
        EvolutionOperation::Recover,
        EvolutionRecovery::ObtainEvidence,
        detail,
    )
}

pub(super) fn owner_error<T>(_: T) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Corruption,
        EvolutionOperation::Recover,
        EvolutionRecovery::Reconcile,
        "durable owner rejected F0 orchestration inputs",
    )
}
