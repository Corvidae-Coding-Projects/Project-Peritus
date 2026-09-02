//! Fresh-process replay and all-or-nothing promotion validation.

use peritus_journal::SqliteJournal;
use peritus_types::{ApprovalRequestId, ProjectId, Sha256Digest};

use crate::{
    ActivationAuthorization, CampaignPhase, CampaignState, EvolutionCampaignId, PendingActivation,
    PointerPhase, ProductionHarnessState, PromotionProposal, recover_campaign, recover_pointer,
};

use super::PromotionQualificationIdentity;
use crate::qualification::identity::{invalid, journal, nominal};

const APPROVAL_STATE_NAMESPACE: u16 = 104;
const PREPARED_EVENTS: u64 = 14;
const COMMITTED_EVENTS: u64 = 16;
const AGGREGATE_HEADS: u64 = 4;

/// Facts reconstructed from C0 by the fresh recovery process.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PromotionQualificationObservation {
    identity: PromotionQualificationIdentity,
    proposal_digest: Sha256Digest,
    authorization_digest: Option<Sha256Digest>,
    campaign_digest: Sha256Digest,
    pointer_digest: Sha256Digest,
    approval_revision: u64,
    approval_position: u64,
    event_count: u64,
    aggregate_heads: u64,
    committed: bool,
}

impl PromotionQualificationObservation {
    /// Deterministic aggregate identities.
    #[must_use]
    pub const fn identity(self) -> PromotionQualificationIdentity {
        self.identity
    }
    /// Exact inert proposal digest.
    #[must_use]
    pub const fn proposal_digest(self) -> Sha256Digest {
        self.proposal_digest
    }
    /// Retained authority digest after activation, or none before activation.
    #[must_use]
    pub const fn authorization_digest(self) -> Option<Sha256Digest> {
        self.authorization_digest
    }
    /// Replayed campaign state digest.
    #[must_use]
    pub const fn campaign_digest(self) -> Sha256Digest {
        self.campaign_digest
    }
    /// Replayed pointer state digest.
    #[must_use]
    pub const fn pointer_digest(self) -> Sha256Digest {
        self.pointer_digest
    }
    /// Durable approval-state revision.
    #[must_use]
    pub const fn approval_revision(self) -> u64 {
        self.approval_revision
    }
    /// Event position that installed the approval state.
    #[must_use]
    pub const fn approval_position(self) -> u64 {
        self.approval_position
    }
    /// Complete C0 event count.
    #[must_use]
    pub const fn event_count(self) -> u64 {
        self.event_count
    }
    /// Complete C0 aggregate-head count.
    #[must_use]
    pub const fn aggregate_heads(self) -> u64 {
        self.aggregate_heads
    }
    /// Whether the exact atomic activation was committed.
    #[must_use]
    pub const fn committed(self) -> bool {
        self.committed
    }
}

/// Rebuilds exact campaign, pointer, and approval state from a fresh journal connection.
///
/// # Errors
/// Rejects any partial or cross-boundary state instead of treating it as an expected crash result.
pub fn observe_promotion(
    owner: &mut SqliteJournal,
    committed: bool,
) -> Result<PromotionQualificationObservation, crate::EvolutionError> {
    let store = owner.store_id();
    let campaign_id =
        EvolutionCampaignId::new(nominal(b"peritus/h1/promotion/campaign/v1\0", store))
            .map_err(|_| invalid("construct qualification campaign identity"))?;
    let project_id = ProjectId::new(nominal(b"peritus/h1/promotion/project/v1\0", store))
        .map_err(|_| invalid("construct qualification project identity"))?;
    let approval_request_id =
        ApprovalRequestId::new(nominal(b"peritus/h1/promotion/approval-request/v1\0", store))
            .map_err(|_| invalid("construct qualification approval request"))?;
    let campaign = recover_campaign(owner, campaign_id)?
        .state()
        .cloned()
        .ok_or_else(|| invalid("qualification campaign state is absent"))?;
    let pointer = recover_pointer(owner, project_id)?
        .state()
        .cloned()
        .ok_or_else(|| invalid("qualification pointer state is absent"))?;
    let proposal =
        campaign.proposal().ok_or_else(|| invalid("qualification campaign proposal is absent"))?;
    let approval = owner
        .state_record(APPROVAL_STATE_NAMESPACE, approval_request_id.as_bytes())
        .map_err(|_| journal())?
        .ok_or_else(|| invalid("qualification approval state is absent"))?;
    let report = owner.integrity_scan().map_err(|_| journal())?;
    validate(&campaign, &pointer, proposal, &approval, &report, committed)?;
    let authorization = pointer.history().last().and_then(crate::ActivationRecord::authorization);
    Ok(PromotionQualificationObservation {
        identity: PromotionQualificationIdentity {
            campaign: campaign_id,
            project: project_id,
            approval_request: approval_request_id,
        },
        proposal_digest: proposal.digest(),
        authorization_digest: authorization.map(ActivationAuthorization::digest),
        campaign_digest: campaign.state_digest(),
        pointer_digest: pointer.state_digest(),
        approval_revision: approval.revision(),
        approval_position: approval.producing_position(),
        event_count: report.event_count(),
        aggregate_heads: report.aggregate_count(),
        committed,
    })
}

fn validate(
    campaign: &CampaignState,
    pointer: &ProductionHarnessState,
    proposal: &PromotionProposal,
    approval: &peritus_journal::DurableStateRecord,
    report: &peritus_journal::IntegrityReport,
    committed: bool,
) -> Result<(), crate::EvolutionError> {
    let expected_events = if committed { COMMITTED_EVENTS } else { PREPARED_EVENTS };
    let base = report.event_count() == expected_events
        && report.last_position() == expected_events
        && report.aggregate_count() == AGGREGATE_HEADS
        && approval.revision() == if committed { 2 } else { 1 }
        && approval.producing_position()
            == if committed { COMMITTED_EVENTS } else { PREPARED_EVENTS };
    let semantic = if committed {
        campaign.phase() == CampaignPhase::Promoted
            && campaign.sequence() == 11
            && campaign.terminal().is_some()
            && pointer.phase() == PointerPhase::Active
            && pointer.sequence() == 3
            && pointer.generation() == 2
            && pointer.current() == proposal.candidate()
            && pointer.pending().is_none()
            && pointer.history().last().is_some_and(|record| {
                record.action_digest() == proposal.digest() && record.authorization().is_some()
            })
    } else {
        campaign.phase() == CampaignPhase::PromotionReview
            && campaign.sequence() == 10
            && campaign.terminal().is_none()
            && pointer.phase() == PointerPhase::PromotionPending
            && pointer.sequence() == 2
            && pointer.generation() == 1
            && pointer.current() == proposal.current()
            && matches!(pointer.pending(), Some(PendingActivation::Promotion(value)) if value == proposal)
            && pointer.history().last().is_some_and(|record| record.authorization().is_none())
    };
    if base && semantic {
        Ok(())
    } else {
        Err(invalid("recovered qualification promotion crosses the atomic commit boundary"))
    }
}
