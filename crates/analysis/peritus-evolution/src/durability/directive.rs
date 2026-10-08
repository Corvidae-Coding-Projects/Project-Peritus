//! Deterministic publication outbox directives for F0 artifacts.

use peritus_journal::{OutboxDraft, OutboxId, OutboxMessage, OutboxState};
use peritus_types::{
    AcceptanceSpecId, CommandId, EvidenceId, Generation, HarnessId, PolicyId, ProjectId,
    ProviderProfileId, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

use crate::{
    ActivationRecord, CampaignCommand, CampaignCommandKind, EvolutionCampaignId, EvolutionError,
    EvolutionErrorKind, EvolutionOperation, EvolutionRecovery, PointerCommand,
    PointerCommandKind, ProductionHarnessState, PromotionProposal,
};

/// Transactional outbox destination for F0 evidence publication.
pub const EVOLUTION_PUBLICATION_DESTINATION: &str = "peritus.evolution.publish.v1";
const DOMAIN: &[u8] = b"PERITUS-F0-PUBLICATION-DIRECTIVE\0";

/// Closed publication class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvolutionPublicationKind {
    /// A campaign promotion decision and evidence bundle.
    CampaignDecision,
    /// A production-pointer promotion or rollback activation.
    HarnessActivation,
}

/// Exact inert request published only through C0's transactional outbox.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvolutionPublicationDirective {
    kind: EvolutionPublicationKind,
    project_id: ProjectId,
    campaign_id: Option<EvolutionCampaignId>,
    revision: RevisionTuple,
    action_digest: Sha256Digest,
    artifact_digest: Sha256Digest,
    evidence_digest: Sha256Digest,
}

impl EvolutionPublicationDirective {
    /// Publication class.
    #[must_use]
    pub const fn kind(self) -> EvolutionPublicationKind {
        self.kind
    }
    /// Project authority.
    #[must_use]
    pub const fn project_id(self) -> ProjectId {
        self.project_id
    }
    /// Source campaign when this is a campaign promotion.
    #[must_use]
    pub const fn campaign_id(self) -> Option<EvolutionCampaignId> {
        self.campaign_id
    }
    /// Exact production revision whose event and evidence are published.
    #[must_use]
    pub const fn revision(self) -> RevisionTuple {
        self.revision
    }
    /// Exact promotion or rollback action digest.
    #[must_use]
    pub const fn action_digest(self) -> Sha256Digest {
        self.action_digest
    }
    /// Finalized content-addressed artifact.
    #[must_use]
    pub const fn artifact_digest(self) -> Sha256Digest {
        self.artifact_digest
    }
    /// Exact semantic evidence digest.
    #[must_use]
    pub const fn evidence_digest(self) -> Sha256Digest {
        self.evidence_digest
    }
    /// Canonical bounded transport bytes.
    #[must_use]
    pub fn canonical_bytes(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(DOMAIN.len() + 130);
        bytes.extend_from_slice(DOMAIN);
        bytes.push(match self.kind {
            EvolutionPublicationKind::CampaignDecision => 1,
            EvolutionPublicationKind::HarnessActivation => 2,
        });
        bytes.extend_from_slice(self.project_id.as_bytes());
        bytes.push(u8::from(self.campaign_id.is_some()));
        if let Some(value) = self.campaign_id {
            bytes.extend_from_slice(value.as_bytes());
        }
        write_revision(&mut bytes, self.revision);
        bytes.extend_from_slice(self.action_digest.as_bytes());
        bytes.extend_from_slice(self.artifact_digest.as_bytes());
        bytes.extend_from_slice(self.evidence_digest.as_bytes());
        bytes
    }

    fn decode(bytes: &[u8]) -> Result<Self, EvolutionError> {
        if !bytes.starts_with(DOMAIN) {
            return Err(protocol("publication directive domain differs"));
        }
        let mut input = &bytes[DOMAIN.len()..];
        let kind = match take_u8(&mut input)? {
            1 => EvolutionPublicationKind::CampaignDecision,
            2 => EvolutionPublicationKind::HarnessActivation,
            _ => return Err(protocol("unknown publication directive kind")),
        };
        let project_id =
            ProjectId::new(take_array(&mut input)?).map_err(|_| protocol("bad project"))?;
        let campaign_id = match take_u8(&mut input)? {
            0 => None,
            1 => Some(
                EvolutionCampaignId::new(take_array(&mut input)?)
                    .map_err(|_| protocol("bad campaign"))?,
            ),
            _ => return Err(protocol("bad optional campaign tag")),
        };
        let revision = read_revision(&mut input)?;
        let action_digest = Sha256Digest::new(take_array(&mut input)?);
        let artifact_digest = Sha256Digest::new(take_array(&mut input)?);
        let evidence_digest = Sha256Digest::new(take_array(&mut input)?);
        if !input.is_empty() {
            return Err(protocol("publication directive has trailing bytes"));
        }
        let value = Self {
            kind,
            project_id,
            campaign_id,
            revision,
            action_digest,
            artifact_digest,
            evidence_digest,
        };
        if value.canonical_bytes() != bytes {
            return Err(protocol("publication directive is noncanonical"));
        }
        Ok(value)
    }

    pub(crate) fn outbox_id(self) -> Result<OutboxId, EvolutionError> {
        let digest = peritus_codec::sha256(&self.canonical_bytes());
        let mut id = [0_u8; 16];
        id.copy_from_slice(&digest.as_bytes()[..16]);
        if id == [0; 16] {
            id[15] = 1;
        }
        OutboxId::new(id).map_err(|_| protocol("derived publication identity is invalid"))
    }
}

/// Exact immutable publication obligation reconstructed from its producing command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvolutionPublicationObligation {
    command_id: CommandId,
    producing_position: u64,
    outbox_id: OutboxId,
    evidence_id: EvidenceId,
    directive: EvolutionPublicationDirective,
}

impl EvolutionPublicationObligation {
    pub(crate) fn new(
        command_id: CommandId,
        producing_position: u64,
        directive: EvolutionPublicationDirective,
    ) -> Result<Self, EvolutionError> {
        if producing_position == 0 {
            return Err(protocol("publication obligation has zero producing position"));
        }
        let outbox_id = directive.outbox_id()?;
        let evidence_id = publication_evidence_id(outbox_id, producing_position)?;
        Ok(Self { command_id, producing_position, outbox_id, evidence_id, directive })
    }

    /// Original command whose immutable transaction created this obligation.
    #[must_use]
    pub const fn command_id(self) -> CommandId {
        self.command_id
    }
    /// Original producing transaction position.
    #[must_use]
    pub const fn producing_position(self) -> u64 {
        self.producing_position
    }
    /// Deterministic persistent outbox identity.
    #[must_use]
    pub const fn outbox_id(self) -> OutboxId {
        self.outbox_id
    }
    /// Deterministic evidence-owner identity.
    #[must_use]
    pub const fn evidence_id(self) -> EvidenceId {
        self.evidence_id
    }
    /// Exact accepted publication semantics.
    #[must_use]
    pub const fn directive(self) -> EvolutionPublicationDirective {
        self.directive
    }
    /// Whether one retained row is the exact transport owner for this obligation.
    #[must_use]
    pub fn matches_message(self, message: &OutboxMessage) -> bool {
        message.id() == self.outbox_id
            && message.producing_position() == self.producing_position
            && message.destination() == EVOLUTION_PUBLICATION_DESTINATION
            && message.payload() == self.directive.canonical_bytes()
    }
}

/// Exact claimed publication row and its delivery fence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvolutionPublicationClaim {
    id: OutboxId,
    fence: u64,
    producing_position: u64,
    directive: EvolutionPublicationDirective,
}

impl EvolutionPublicationClaim {
    /// Validates and decodes one claimed C0 publication row.
    ///
    /// # Errors
    /// Rejects an unclaimed row, wrong destination, malformed payload, or mismatched identity.
    pub fn from_message(message: &OutboxMessage) -> Result<Self, EvolutionError> {
        let fence = message
            .fence()
            .filter(|_| message.state() == OutboxState::Claimed)
            .ok_or_else(|| protocol("publication message is not claimed"))?;
        if message.destination() != EVOLUTION_PUBLICATION_DESTINATION {
            return Err(protocol("publication destination differs"));
        }
        let directive = EvolutionPublicationDirective::decode(message.payload())?;
        if directive.outbox_id()? != message.id() {
            return Err(protocol("publication outbox identity differs"));
        }
        Ok(Self {
            id: message.id(),
            fence,
            producing_position: message.producing_position(),
            directive,
        })
    }
    /// Exact outbox identity.
    #[must_use]
    pub const fn id(&self) -> OutboxId {
        self.id
    }
    /// Positive current claim fence.
    #[must_use]
    pub const fn fence(&self) -> u64 {
        self.fence
    }
    /// Journal position that created the publication request.
    #[must_use]
    pub const fn producing_position(&self) -> u64 {
        self.producing_position
    }
    /// Checked inert directive.
    #[must_use]
    pub const fn directive(&self) -> EvolutionPublicationDirective {
        self.directive
    }
}

pub(super) fn campaign_outbox(
    command: &CampaignCommand,
) -> Result<Vec<OutboxDraft>, EvolutionError> {
    let CampaignCommandKind::RequestPromotion(proposal) = command.kind() else {
        return Ok(Vec::new());
    };
    outbox_draft(campaign_publication_directive(proposal))
    .map(|value| vec![value])
}

pub(super) fn pointer_outbox(
    command: &PointerCommand,
    state: &ProductionHarnessState,
) -> Result<Vec<OutboxDraft>, EvolutionError> {
    if !matches!(
        command.kind(),
        PointerCommandKind::ActivatePromotion { .. } | PointerCommandKind::ActivateRollback { .. }
    ) {
        return Ok(Vec::new());
    }
    let record = state
        .history()
        .last()
        .ok_or_else(|| protocol("activated pointer has no activation record"))?;
    outbox_draft(activation_publication_directive(state.project_id(), record))
    .map(|value| vec![value])
}

pub(crate) const fn campaign_publication_directive(
    proposal: &PromotionProposal,
) -> EvolutionPublicationDirective {
    EvolutionPublicationDirective {
        kind: EvolutionPublicationKind::CampaignDecision,
        project_id: proposal.project_id(),
        campaign_id: Some(proposal.campaign_id()),
        revision: proposal.candidate().revision(),
        action_digest: proposal.digest(),
        artifact_digest: proposal.evidence_bundle_artifact(),
        evidence_digest: proposal.digest(),
    }
}

pub(crate) const fn activation_publication_directive(
    project_id: ProjectId,
    record: &ActivationRecord,
) -> EvolutionPublicationDirective {
    EvolutionPublicationDirective {
        kind: EvolutionPublicationKind::HarnessActivation,
        project_id,
        campaign_id: record.campaign_id(),
        revision: record.successor().revision(),
        action_digest: record.action_digest(),
        artifact_digest: record.evidence_artifact(),
        evidence_digest: record.evidence_digest(),
    }
}

pub(crate) fn outbox_draft(
    value: EvolutionPublicationDirective,
) -> Result<OutboxDraft, EvolutionError> {
    OutboxDraft::persistent(
        value.outbox_id()?,
        EVOLUTION_PUBLICATION_DESTINATION.to_owned(),
        value.canonical_bytes(),
    )
    .map_err(|_| protocol("publication outbox draft is invalid"))
}

pub(crate) fn publication_evidence_id(
    outbox_id: OutboxId,
    producing_position: u64,
) -> Result<EvidenceId, EvolutionError> {
    if producing_position == 0 {
        return Err(protocol("publication evidence position is zero"));
    }
    let mut bytes = b"PERITUS-F0-EVIDENCE-ID\0".to_vec();
    bytes.extend_from_slice(outbox_id.as_bytes());
    bytes.extend_from_slice(&producing_position.to_be_bytes());
    let digest = peritus_codec::sha256(&bytes);
    let mut id = [0_u8; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    if id == [0; 16] {
        id[15] = 1;
    }
    EvidenceId::new(id).map_err(|_| protocol("derived evidence identity is invalid"))
}

fn take_u8(input: &mut &[u8]) -> Result<u8, EvolutionError> {
    let (&value, tail) = input.split_first().ok_or_else(|| protocol("truncated directive"))?;
    *input = tail;
    Ok(value)
}

fn take_array<const N: usize>(input: &mut &[u8]) -> Result<[u8; N], EvolutionError> {
    if input.len() < N {
        return Err(protocol("truncated directive"));
    }
    let (value, tail) = input.split_at(N);
    *input = tail;
    value.try_into().map_err(|_| protocol("invalid directive field width"))
}

fn write_revision(bytes: &mut Vec<u8>, revision: RevisionTuple) {
    bytes.extend_from_slice(revision.acceptance_spec_id().as_bytes());
    bytes.extend_from_slice(revision.harness_id().as_bytes());
    bytes.extend_from_slice(revision.workspace_id().as_bytes());
    bytes.extend_from_slice(&revision.workspace_generation().get().to_be_bytes());
    bytes.extend_from_slice(&revision.workspace_revision().get().to_be_bytes());
    bytes.extend_from_slice(revision.policy_id().as_bytes());
    bytes.extend_from_slice(revision.provider_profile_id().as_bytes());
}

fn read_revision(input: &mut &[u8]) -> Result<RevisionTuple, EvolutionError> {
    Ok(RevisionTuple::new(
        AcceptanceSpecId::new(take_array(input)?).map_err(|_| protocol("bad acceptance spec"))?,
        HarnessId::new(take_array(input)?).map_err(|_| protocol("bad harness"))?,
        WorkspaceId::new(take_array(input)?).map_err(|_| protocol("bad workspace"))?,
        Generation::new(take_u64(input)?).map_err(|_| protocol("bad workspace generation"))?,
        RevisionNumber::new(take_u64(input)?).map_err(|_| protocol("bad workspace revision"))?,
        PolicyId::new(take_array(input)?).map_err(|_| protocol("bad policy"))?,
        ProviderProfileId::new(take_array(input)?).map_err(|_| protocol("bad provider profile"))?,
    ))
}

fn take_u64(input: &mut &[u8]) -> Result<u64, EvolutionError> {
    Ok(u64::from_be_bytes(take_array(input)?))
}

const fn protocol(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Codec,
        EvolutionOperation::Publish,
        EvolutionRecovery::Quarantine,
        detail,
    )
}
