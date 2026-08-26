//! Atomic promotion/rollback commit with approve-once consumption.

use peritus_approval::ApprovalUseOutcome;
use peritus_codec::{CodecLimits, encode_message};
use peritus_journal::{
    AppendRequest, ApprovalUseCommitRequest, ApprovalUseResolution, ApprovalUseResolutionRequest,
    CommittedApprovalUse, CurrentCredentialRegistry, EventDraft, HeadExpectation, SqliteJournal,
};

use crate::{
    CampaignCommand, CampaignCommandKind, CampaignTransition, EvolutionError, PointerCommand,
    PointerCommandKind, PointerTransition,
    wire::{CampaignCommandFrame, PointerCommandFrame},
};

use super::{
    CAMPAIGN_STATE_NAMESPACE, POINTER_STATE_NAMESPACE, binding,
    campaign::{
        artifact_dependencies as campaign_artifacts, codec, journal_error, recovery,
        validate_current as validate_campaign_current,
    },
    campaign_aggregate_key, campaign_state_key,
    directive::pointer_outbox,
    pointer::{
        artifact_dependencies as pointer_artifacts, validate_current as validate_pointer_current,
    },
    pointer_aggregate_key, pointer_state_key,
};

mod frames;

use frames::{campaign_event, campaign_install, pointer_event, pointer_install};

/// One complete already-decided atomic pointer mutation.
pub struct AtomicActivation<'a> {
    campaign: Option<(&'a CampaignCommand, &'a CampaignTransition)>,
    pointer_command: &'a PointerCommand,
    pointer_transition: &'a PointerTransition,
    approval: ApprovalUseOutcome,
    expected_approval_revision: u64,
    registry: &'a CurrentCredentialRegistry,
}

impl<'a> AtomicActivation<'a> {
    /// Binds a campaign terminal and pointer promotion to one approval consumption.
    ///
    /// # Errors
    /// Rejects any campaign, pointer, action, or approval binding disagreement.
    #[allow(clippy::too_many_arguments)]
    pub fn promotion(
        campaign_command: &'a CampaignCommand,
        campaign_transition: &'a CampaignTransition,
        pointer_command: &'a PointerCommand,
        pointer_transition: &'a PointerTransition,
        approval: ApprovalUseOutcome,
        expected_approval_revision: u64,
        registry: &'a CurrentCredentialRegistry,
    ) -> Result<Self, EvolutionError> {
        binding::validate_campaign(campaign_command, campaign_transition)?;
        binding::validate_pointer(pointer_command, pointer_transition)?;
        validate_promotion_pair(
            campaign_command,
            campaign_transition,
            pointer_command,
            pointer_transition,
            &approval,
        )?;
        validate_approval_reuse(pointer_transition, &approval)?;
        Ok(Self {
            campaign: Some((campaign_command, campaign_transition)),
            pointer_command,
            pointer_transition,
            approval,
            expected_approval_revision,
            registry,
        })
    }

    /// Binds a retained-target rollback to its new independent approval consumption.
    ///
    /// # Errors
    /// Rejects a non-rollback transition, mismatched approval, or reused approval digest.
    pub fn rollback(
        pointer_command: &'a PointerCommand,
        pointer_transition: &'a PointerTransition,
        approval: ApprovalUseOutcome,
        expected_approval_revision: u64,
        registry: &'a CurrentCredentialRegistry,
    ) -> Result<Self, EvolutionError> {
        binding::validate_pointer(pointer_command, pointer_transition)?;
        if !matches!(pointer_command.kind(), PointerCommandKind::ActivateRollback { .. }) {
            return Err(binding::binding("atomic rollback requires ActivateRollback"));
        }
        validate_action_authority(pointer_command, pointer_transition, &approval)?;
        validate_approval_reuse(pointer_transition, &approval)?;
        Ok(Self {
            campaign: None,
            pointer_command,
            pointer_transition,
            approval,
            expected_approval_revision,
            registry,
        })
    }
}

/// Commits all F0 heads/checkpoints and approval consumption in one C0 transaction.
///
/// # Errors
/// Rejects stale heads/state, invalid frames, missing artifacts, stale registry/approval state, or
/// any journal commit failure without partially committing the requested activation.
pub fn commit_atomic_activation(
    journal: &mut SqliteJournal,
    activation: AtomicActivation<'_>,
) -> Result<CommittedApprovalUse, EvolutionError> {
    let AtomicActivation {
        campaign,
        pointer_command,
        pointer_transition,
        mut approval,
        expected_approval_revision,
        registry,
    } = activation;
    let pointer_aggregate = pointer_aggregate_key(pointer_command.project_id())?;
    let pointer_key = pointer_state_key(pointer_command.project_id());
    let mut events = vec![pointer_event(pointer_aggregate, pointer_transition)?];
    let mut installs = vec![pointer_install(
        pointer_key.clone(),
        pointer_command.expected_sequence(),
        pointer_transition,
    )?];
    let mut artifacts = pointer_artifacts(pointer_command.kind());
    if let Some(record) = pointer_transition.state().history().last() {
        artifacts.push(peritus_journal::ArtifactDependency::new(record.evidence_artifact()));
    }

    let pointer_command_bytes = encode_message(
        &PointerCommandFrame::from_command(pointer_command).map_err(codec)?,
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    let mut request_preimage = b"PERITUS-F0-ATOMIC-ACTIVATION\0".to_vec();
    request_preimage.extend_from_slice(&pointer_command_bytes);
    let command_id = pointer_command.command_id();

    if let Some((campaign_command, campaign_transition)) = campaign {
        let campaign_aggregate = campaign_aggregate_key(campaign_command.campaign_id())?;
        let campaign_key = campaign_state_key(campaign_command.campaign_id());
        events.push(campaign_event(campaign_aggregate, campaign_transition)?);
        installs.push(campaign_install(
            campaign_key,
            campaign_command.expected_sequence(),
            campaign_transition,
        )?);
        artifacts.extend(campaign_artifacts(campaign_command.kind()));
        let bytes = encode_message(
            &CampaignCommandFrame::from_command(campaign_command).map_err(codec)?,
            CodecLimits::PRODUCTION,
        )
        .map_err(codec)?;
        request_preimage.extend_from_slice(&bytes);
    }

    events.sort_by_key(EventDraft::aggregate);
    installs.sort_by(|left, right| {
        (left.namespace(), left.key()).cmp(&(right.namespace(), right.key()))
    });
    artifacts.sort_unstable();
    artifacts.dedup();
    let base_request_digest = peritus_codec::sha256(&request_preimage);
    let resolution = ApprovalUseResolutionRequest::new(
        command_id,
        base_request_digest,
        installs.clone(),
        &approval,
        expected_approval_revision,
        registry,
    )
    .map_err(journal_error)?;
    match journal.resolve_approval_use(&resolution, approval).map_err(journal_error)? {
        ApprovalUseResolution::Committed(committed) => {
            validate_resolved_activation(committed.batch(), &events, &artifacts)?;
            return Ok(*committed);
        }
        ApprovalUseResolution::DefinitelyAbsent(outcome) => approval = *outcome,
    }

    let pointer_head = journal.head(pointer_aggregate).map_err(journal_error)?;
    let pointer_current =
        journal.state_record(POINTER_STATE_NAMESPACE, &pointer_key).map_err(journal_error)?;
    validate_pointer_current(pointer_command, pointer_head, pointer_current.as_ref())?;
    let mut heads = vec![expectation(pointer_aggregate, pointer_head)];
    if let Some((campaign_command, _)) = campaign {
        let campaign_aggregate = campaign_aggregate_key(campaign_command.campaign_id())?;
        let campaign_key = campaign_state_key(campaign_command.campaign_id());
        let campaign_head = journal.head(campaign_aggregate).map_err(journal_error)?;
        let campaign_current =
            journal.state_record(CAMPAIGN_STATE_NAMESPACE, &campaign_key).map_err(journal_error)?;
        validate_campaign_current(campaign_command, campaign_head, campaign_current.as_ref())?;
        heads.push(expectation(campaign_aggregate, campaign_head));
    }
    heads.sort_by_key(|value| value.key());
    let request = AppendRequest::new(
        journal.store_id(),
        command_id,
        base_request_digest,
        heads,
        events,
        installs,
        artifacts,
        None,
        None,
        pointer_outbox(pointer_command, pointer_transition.state())?,
    );
    let request =
        ApprovalUseCommitRequest::new(request, approval, expected_approval_revision, registry)
            .map_err(journal_error)?;
    journal.commit_approval_use(request).map_err(journal_error)
}

fn validate_resolved_activation(
    batch: &peritus_journal::CommittedBatch,
    events: &[EventDraft],
    artifacts: &[peritus_journal::ArtifactDependency],
) -> Result<(), EvolutionError> {
    if batch.records().len() != events.len()
        || batch.records().iter().zip(events).any(|(record, event)| {
            record.aggregate() != event.aggregate() || record.frame_bytes() != event.frame().bytes()
        })
        || batch.artifact_dependencies() != artifacts
    {
        return Err(recovery("resolved atomic activation differs from its exact durable effects"));
    }
    Ok(())
}

fn validate_promotion_pair(
    campaign_command: &CampaignCommand,
    campaign_transition: &CampaignTransition,
    pointer_command: &PointerCommand,
    pointer_transition: &PointerTransition,
    approval: &ApprovalUseOutcome,
) -> Result<(), EvolutionError> {
    let CampaignCommandKind::ActivatePromotion { activation_digest } = campaign_command.kind()
    else {
        return Err(binding::binding("atomic promotion requires campaign activation"));
    };
    let PointerCommandKind::ActivatePromotion { campaign_terminal_digest, authorization, .. } =
        pointer_command.kind()
    else {
        return Err(binding::binding("atomic promotion requires pointer activation"));
    };
    let proposal = campaign_transition
        .state()
        .proposal()
        .ok_or_else(|| binding::binding("promoted campaign lost its proposal"))?;
    if campaign_command.command_id() != pointer_command.command_id()
        || campaign_transition.state().project_id() != pointer_transition.state().project_id()
        || proposal.id()
            != match pointer_command.kind() {
                PointerCommandKind::ActivatePromotion { promotion_id, .. } => *promotion_id,
                _ => return Err(binding::binding("pointer command changed activation kind")),
            }
        || proposal.digest() != authorization.action_digest()
        || *activation_digest != authorization.digest()
        || *campaign_terminal_digest != campaign_transition.state().state_digest()
    {
        return Err(binding::binding("campaign, pointer, and authority activation facts differ"));
    }
    validate_action_authority(pointer_command, pointer_transition, approval)
}

fn validate_action_authority(
    command: &PointerCommand,
    transition: &PointerTransition,
    approval: &ApprovalUseOutcome,
) -> Result<(), EvolutionError> {
    let authorization = match command.kind() {
        PointerCommandKind::ActivatePromotion { authorization, .. }
        | PointerCommandKind::ActivateRollback { authorization, .. } => *authorization,
        _ => return Err(binding::binding("atomic activation command has no authority")),
    };
    let approved = approval.transition();
    let request = approval.aggregate().request();
    let action_digest = peritus_approval::ActionDigest::from_sha256(authorization.action_digest());
    if approved.action_id() != request.action_id()
        || approved.action_digest() != action_digest
        || request.action_digest() != action_digest
        || authorization.approval_use_digest() != crate::runtime::approval_use_digest(approval)
        || transition.state().history().last().and_then(crate::ActivationRecord::authorization)
            != Some(authorization)
    {
        return Err(binding::binding("approval consumption differs from pointer authorization"));
    }
    Ok(())
}

fn validate_approval_reuse(
    transition: &PointerTransition,
    approval: &ApprovalUseOutcome,
) -> Result<(), EvolutionError> {
    let digest = crate::runtime::approval_use_digest(approval);
    let history = transition.state().history();
    if history.is_empty()
        || history[..history.len() - 1]
            .iter()
            .filter_map(crate::ActivationRecord::authorization)
            .any(|authorization| authorization.approval_use_digest() == digest)
    {
        return Err(binding::binding("approval use was already retained in pointer history"));
    }
    Ok(())
}

fn expectation(
    aggregate: peritus_journal::AggregateKey,
    head: Option<peritus_journal::AggregateHead>,
) -> HeadExpectation {
    head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present)
}
