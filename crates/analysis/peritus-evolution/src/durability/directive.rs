//! Deterministic publication outbox directives for F0 artifacts.

use peritus_journal::{OutboxDraft, OutboxId, OutboxMessage, OutboxState};
use peritus_types::{ProjectId, Sha256Digest};

use crate::{
    CampaignCommand, CampaignCommandKind, EvolutionCampaignId, EvolutionError, EvolutionErrorKind,
    EvolutionOperation, EvolutionRecovery, PointerCommand, PointerCommandKind,
    ProductionHarnessState,
};

/// Transactional outbox destination for F0 evidence publication.
pub const EVOLUTION_PUBLICATION_DESTINATION: &str = "peritus.evolution.publish.v1";
const MAX_ATTEMPTS: u16 = 16;
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
        let action_digest = Sha256Digest::new(take_array(&mut input)?);
        let artifact_digest = Sha256Digest::new(take_array(&mut input)?);
        let evidence_digest = Sha256Digest::new(take_array(&mut input)?);
        if !input.is_empty() {
            return Err(protocol("publication directive has trailing bytes"));
        }
        let value =
            Self { kind, project_id, campaign_id, action_digest, artifact_digest, evidence_digest };
        if value.canonical_bytes() != bytes {
            return Err(protocol("publication directive is noncanonical"));
        }
        Ok(value)
    }

    fn outbox_id(self) -> Result<OutboxId, EvolutionError> {
        let digest = peritus_codec::sha256(&self.canonical_bytes());
        let mut id = [0_u8; 16];
        id.copy_from_slice(&digest.as_bytes()[..16]);
        if id == [0; 16] {
            id[15] = 1;
        }
        OutboxId::new(id).map_err(|_| protocol("derived publication identity is invalid"))
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
    draft(EvolutionPublicationDirective {
        kind: EvolutionPublicationKind::CampaignDecision,
        project_id: proposal.project_id(),
        campaign_id: Some(proposal.campaign_id()),
        action_digest: proposal.digest(),
        artifact_digest: proposal.evidence_bundle_artifact(),
        evidence_digest: proposal.digest(),
    })
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
    draft(EvolutionPublicationDirective {
        kind: EvolutionPublicationKind::HarnessActivation,
        project_id: state.project_id(),
        campaign_id: record.campaign_id(),
        action_digest: record.action_digest(),
        artifact_digest: record.evidence_artifact(),
        evidence_digest: record.evidence_digest(),
    })
    .map(|value| vec![value])
}

fn draft(value: EvolutionPublicationDirective) -> Result<OutboxDraft, EvolutionError> {
    OutboxDraft::new(
        value.outbox_id()?,
        EVOLUTION_PUBLICATION_DESTINATION.to_owned(),
        value.canonical_bytes(),
        MAX_ATTEMPTS,
    )
    .map_err(|_| protocol("publication outbox draft is invalid"))
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

const fn protocol(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Codec,
        EvolutionOperation::Publish,
        EvolutionRecovery::Quarantine,
        detail,
    )
}
