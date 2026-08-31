//! Complete production promotion setup and atomic-boundary observation.

mod observation;
mod setup;

pub use observation::{PromotionQualificationObservation, observe_promotion};

use peritus_artifact_store::ArtifactStore;
use peritus_journal::{CommittedApprovalUse, SqliteJournal};
use peritus_types::{ApprovalRequestId, ProjectId, Sha256Digest};

use crate::{
    ActivationAuthorization, AtomicActivation, CampaignCommand, CampaignCommandKind,
    CampaignTransition, EvolutionCampaignId, PointerCommand, PointerCommandKind, PointerTransition,
    commit_atomic_activation, decide_campaign, decide_pointer,
};

use super::{
    approval, evidence,
    harness::HarnessFixture,
    identity::{command, digest, event, invalid, journal},
};
use setup::{
    finalize_artifacts, next_campaign_command, next_pointer_command, seed_campaign, seed_pointer,
};

/// Deterministic aggregate identities used by both crash processes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PromotionQualificationIdentity {
    campaign: EvolutionCampaignId,
    project: ProjectId,
    approval_request: ApprovalRequestId,
}

impl PromotionQualificationIdentity {
    /// Qualification campaign aggregate.
    #[must_use]
    pub const fn campaign_id(self) -> EvolutionCampaignId {
        self.campaign
    }
    /// Qualification production-pointer aggregate.
    #[must_use]
    pub const fn project_id(self) -> ProjectId {
        self.project
    }
    /// Qualification approve-once state key.
    #[must_use]
    pub const fn approval_request_id(self) -> ApprovalRequestId {
        self.approval_request
    }
}

/// Final promotion transition held only in the first process until explicitly committed.
pub struct PreparedPromotion {
    identity: PromotionQualificationIdentity,
    proposal_digest: Sha256Digest,
    authorization_digest: Sha256Digest,
    campaign_before: Sha256Digest,
    pointer_before: Sha256Digest,
    campaign_after: Sha256Digest,
    pointer_after: Sha256Digest,
    campaign_command: CampaignCommand,
    campaign_transition: CampaignTransition,
    pointer_command: PointerCommand,
    pointer_transition: PointerTransition,
    approval: peritus_approval::ApprovalUseOutcome,
    expected_approval_revision: u64,
    registry: peritus_journal::CurrentCredentialRegistry,
}

impl PreparedPromotion {
    /// Deterministic aggregate identities.
    #[must_use]
    pub const fn identity(&self) -> PromotionQualificationIdentity {
        self.identity
    }
    /// Exact inert promotion action digest.
    #[must_use]
    pub const fn proposal_digest(&self) -> Sha256Digest {
        self.proposal_digest
    }
    /// Exact admitted authority digest.
    #[must_use]
    pub const fn authorization_digest(&self) -> Sha256Digest {
        self.authorization_digest
    }
    /// Durable campaign digest immediately before activation.
    #[must_use]
    pub const fn campaign_before(&self) -> Sha256Digest {
        self.campaign_before
    }
    /// Durable pointer digest immediately before activation.
    #[must_use]
    pub const fn pointer_before(&self) -> Sha256Digest {
        self.pointer_before
    }
    /// Accepted campaign successor digest.
    #[must_use]
    pub const fn campaign_after(&self) -> Sha256Digest {
        self.campaign_after
    }
    /// Accepted pointer successor digest.
    #[must_use]
    pub const fn pointer_after(&self) -> Sha256Digest {
        self.pointer_after
    }

    /// Atomically commits both activation events, both checkpoints, and approval consumption.
    ///
    /// # Errors
    /// Returns the production F0 error when any campaign, pointer, artifact, registry, approval,
    /// or journal fence rejects the complete transaction.
    pub fn commit(
        self,
        owner: &mut SqliteJournal,
    ) -> Result<CommittedPromotion, crate::EvolutionError> {
        let Self {
            identity,
            proposal_digest,
            authorization_digest,
            campaign_before,
            pointer_before,
            campaign_after,
            pointer_after,
            campaign_command,
            campaign_transition,
            pointer_command,
            pointer_transition,
            approval,
            expected_approval_revision,
            registry,
        } = self;
        let activation = AtomicActivation::promotion(
            &campaign_command,
            &campaign_transition,
            &pointer_command,
            &pointer_transition,
            approval,
            expected_approval_revision,
            &registry,
        )?;
        let committed = commit_atomic_activation(owner, activation)?;
        Ok(CommittedPromotion {
            identity,
            proposal_digest,
            authorization_digest,
            campaign_before,
            pointer_before,
            campaign_after,
            pointer_after,
            committed,
        })
    }
}

/// Receipt retained after the exact atomic transaction returned and before caller acknowledgement.
pub struct CommittedPromotion {
    identity: PromotionQualificationIdentity,
    proposal_digest: Sha256Digest,
    authorization_digest: Sha256Digest,
    campaign_before: Sha256Digest,
    pointer_before: Sha256Digest,
    campaign_after: Sha256Digest,
    pointer_after: Sha256Digest,
    committed: CommittedApprovalUse,
}

impl CommittedPromotion {
    /// Deterministic aggregate identities.
    #[must_use]
    pub const fn identity(&self) -> PromotionQualificationIdentity {
        self.identity
    }
    /// Exact inert promotion action digest.
    #[must_use]
    pub const fn proposal_digest(&self) -> Sha256Digest {
        self.proposal_digest
    }
    /// Exact admitted authority digest.
    #[must_use]
    pub const fn authorization_digest(&self) -> Sha256Digest {
        self.authorization_digest
    }
    /// Durable campaign predecessor digest.
    #[must_use]
    pub const fn campaign_before(&self) -> Sha256Digest {
        self.campaign_before
    }
    /// Durable pointer predecessor digest.
    #[must_use]
    pub const fn pointer_before(&self) -> Sha256Digest {
        self.pointer_before
    }
    /// Committed campaign successor digest.
    #[must_use]
    pub const fn campaign_after(&self) -> Sha256Digest {
        self.campaign_after
    }
    /// Committed pointer successor digest.
    #[must_use]
    pub const fn pointer_after(&self) -> Sha256Digest {
        self.pointer_after
    }
    /// Complete C0 atomic receipt.
    #[must_use]
    pub const fn committed(&self) -> &CommittedApprovalUse {
        &self.committed
    }
}

/// Seeds every real prerequisite and accepts, but does not commit, the final atomic activation.
///
/// # Errors
/// Rejects a nonempty journal, invalid fixture binding, unavailable artifact finalization, or any
/// production prerequisite commit failure.
pub fn prepare_promotion(
    owner: &mut SqliteJournal,
    artifacts: &ArtifactStore,
) -> Result<PreparedPromotion, crate::EvolutionError> {
    let report = owner.integrity_scan().map_err(|_| journal())?;
    if report.event_count() != 0 || report.aggregate_count() != 0 {
        return Err(invalid("promotion qualification journal is not empty"));
    }
    let store = owner.store_id();
    let fixture = HarnessFixture::build(store)?;
    let artifacts = finalize_artifacts(artifacts, store)?;
    let evidence = evidence::build(&fixture, &artifacts, store)?;
    let campaign = seed_campaign(owner, &fixture, &artifacts, &evidence, store)?;
    let pointer = seed_pointer(owner, &fixture, &artifacts, &evidence.proposal, store)?;
    let approval =
        approval::prepare(owner, &evidence.proposal, fixture.baseline.revision(), store)?;
    let authorization = ActivationAuthorization::new(
        evidence.proposal.digest(),
        digest(b"peritus/h1/promotion/dispatch/v1\0", store),
        digest(b"peritus/h1/promotion/capability-use/v1\0", store),
        crate::runtime::approval_use_digest(&approval.outcome),
        digest(b"peritus/h1/promotion/authority/v1\0", store),
    );
    let shared_command = command(b"peritus/h1/promotion/atomic-command/v1\0", store)?;
    let campaign_command = next_campaign_command(
        &campaign,
        shared_command,
        event(b"peritus/h1/promotion/campaign-activation/v1\0", store)?,
        CampaignCommandKind::ActivatePromotion { activation_digest: authorization.digest() },
    )?;
    let campaign_transition = decide_campaign(Some(&campaign), &campaign_command)?;
    let pointer_command = next_pointer_command(
        &pointer,
        shared_command,
        event(b"peritus/h1/promotion/pointer-activation/v1\0", store)?,
        PointerCommandKind::ActivatePromotion {
            promotion_id: evidence.proposal.id(),
            campaign_terminal_digest: campaign_transition.state().state_digest(),
            authorization,
        },
    )?;
    let pointer_transition = decide_pointer(Some(&pointer), &pointer_command)?;
    let identity = PromotionQualificationIdentity {
        campaign: campaign.campaign_id(),
        project: pointer.project_id(),
        approval_request: approval.request_id,
    };
    Ok(PreparedPromotion {
        identity,
        proposal_digest: evidence.proposal.digest(),
        authorization_digest: authorization.digest(),
        campaign_before: campaign.state_digest(),
        pointer_before: pointer.state_digest(),
        campaign_after: campaign_transition.state().state_digest(),
        pointer_after: pointer_transition.state().state_digest(),
        campaign_command,
        campaign_transition,
        pointer_command,
        pointer_transition,
        approval: approval.outcome,
        expected_approval_revision: 1,
        registry: approval.current,
    })
}
