//! Real prerequisite commits and accepted final transitions.

use peritus_artifact_store::ArtifactStore;
use peritus_codec::{CodecLimits, decode_message, encode_message};
use peritus_journal::{SqliteJournal, StoreId};
use peritus_types::{CommandId, EventId, ProjectId, Sha256Digest};

use crate::{
    CampaignCommand, CampaignCommandKind, CampaignState, EvolutionCampaignId, EvolutionLimits,
    PointerCommand, PointerCommandKind, ProductionHarnessState, PromotionProposal,
    commit_campaign_transition, commit_pointer_transition, decide_campaign, decide_pointer,
    finalize_evolution_artifact, recover_campaign,
    wire::{CampaignCommandFrame, CampaignEventFrame, CampaignStateFrame},
};

use super::super::{
    evidence::{PromotionEvidence, QualificationArtifacts},
    harness::HarnessFixture,
    identity::{command, digest, event, invalid, nominal},
};

pub(super) fn seed_campaign(
    owner: &mut SqliteJournal,
    fixture: &HarnessFixture,
    artifacts: &QualificationArtifacts,
    evidence: &PromotionEvidence,
    store: StoreId,
) -> Result<CampaignState, crate::EvolutionError> {
    let campaign_id =
        EvolutionCampaignId::new(nominal(b"peritus/h1/promotion/campaign/v1\0", store))
            .map_err(|_| invalid("construct qualification campaign identity"))?;
    let project_id = ProjectId::new(nominal(b"peritus/h1/promotion/project/v1\0", store))
        .map_err(|_| invalid("construct qualification project identity"))?;
    let policy_digest = fixture.policy.policy().digest();
    let kinds = vec![
        CampaignCommandKind::CreateCampaign {
            project_id,
            baseline: fixture.baseline,
            policy: fixture.policy.clone(),
            limits: EvolutionLimits::compiled(),
        },
        CampaignCommandKind::FreezeCampaign,
        CampaignCommandKind::RecordBaselineEvidence {
            artifact_digest: artifacts.baseline,
            evidence_digest: artifacts.baseline_evidence,
        },
        CampaignCommandKind::SubmitDiagnosis(evidence.diagnosis.clone()),
        CampaignCommandKind::AdmitChangeManifest(evidence.manifest.clone()),
        CampaignCommandKind::AdmitVariant(evidence.variant.clone()),
        CampaignCommandKind::AdmitEvaluation {
            variant_id: evidence.variant.id(),
            evidence: evidence.evaluation.clone(),
        },
        CampaignCommandKind::CompleteAttribution {
            attribution: evidence.attribution.clone(),
            assessment: evidence.assessment.clone(),
        },
        CampaignCommandKind::RecordSelection(evidence.selection.clone()),
        CampaignCommandKind::RequestPromotion(evidence.proposal.clone()),
    ];
    let mut state = None;
    for (index, kind) in kinds.into_iter().enumerate() {
        let step = u8::try_from(index + 1)
            .map_err(|_| invalid("qualification campaign step overflows"))?;
        let command =
            campaign_command(state.as_ref(), campaign_id, policy_digest, kind, step, store)?;
        let transition = decide_campaign(state.as_ref(), &command)?;
        validate_campaign_frames(&command, &transition, step)?;
        commit_campaign_transition(owner, &command, &transition)?;
        recover_campaign(owner, campaign_id).map_err(|_| invalid(campaign_replay_error(step)))?;
        state = Some(transition.state().clone());
    }
    state.ok_or_else(|| invalid("qualification campaign was not seeded"))
}

fn validate_campaign_frames(
    command: &CampaignCommand,
    transition: &crate::CampaignTransition,
    step: u8,
) -> Result<(), crate::EvolutionError> {
    validate_campaign_frames_inner(command, transition)
        .map_err(|()| invalid(campaign_frame_error(step)))
}

fn validate_campaign_frames_inner(
    command: &CampaignCommand,
    transition: &crate::CampaignTransition,
) -> Result<(), ()> {
    let command = CampaignCommandFrame::from_command(command).map_err(|_| ())?;
    let command = encode_message(&command, CodecLimits::PRODUCTION).map_err(|_| ())?;
    decode_message::<CampaignCommandFrame>(&command, CodecLimits::PRODUCTION).map_err(|_| ())?;
    let event = CampaignEventFrame::from_event(transition.event()).map_err(|_| ())?;
    let event = encode_message(&event, CodecLimits::PRODUCTION).map_err(|_| ())?;
    decode_message::<CampaignEventFrame>(&event, CodecLimits::PRODUCTION).map_err(|_| ())?;
    let state = CampaignStateFrame::from_state(transition.state()).map_err(|_| ())?;
    let state = encode_message(&state, CodecLimits::PRODUCTION).map_err(|_| ())?;
    decode_message::<CampaignStateFrame>(&state, CodecLimits::PRODUCTION).map_err(|_| ())?;
    Ok(())
}

const fn campaign_frame_error(step: u8) -> &'static str {
    match step {
        1 => "qualification create-campaign frame is not canonical",
        2 => "qualification freeze-campaign frame is not canonical",
        3 => "qualification baseline-evidence frame is not canonical",
        4 => "qualification diagnosis frame is not canonical",
        5 => "qualification change-manifest frame is not canonical",
        6 => "qualification variant frame is not canonical",
        7 => "qualification evaluation frame is not canonical",
        8 => "qualification attribution frame is not canonical",
        9 => "qualification selection frame is not canonical",
        10 => "qualification promotion-proposal frame is not canonical",
        _ => "qualification campaign frame is not canonical",
    }
}

const fn campaign_replay_error(step: u8) -> &'static str {
    match step {
        1 => "qualification create-campaign replay differs",
        2 => "qualification freeze-campaign replay differs",
        3 => "qualification baseline-evidence replay differs",
        4 => "qualification diagnosis replay differs",
        5 => "qualification change-manifest replay differs",
        6 => "qualification variant replay differs",
        7 => "qualification evaluation replay differs",
        8 => "qualification attribution replay differs",
        9 => "qualification selection replay differs",
        10 => "qualification promotion-proposal replay differs",
        _ => "qualification campaign replay differs",
    }
}

pub(super) fn seed_pointer(
    owner: &mut SqliteJournal,
    fixture: &HarnessFixture,
    artifacts: &QualificationArtifacts,
    proposal: &PromotionProposal,
    store: StoreId,
) -> Result<ProductionHarnessState, crate::EvolutionError> {
    let genesis = PointerCommand::new(
        command(b"peritus/h1/promotion/pointer-command-1/v1\0", store)?,
        event(b"peritus/h1/promotion/pointer-event-1/v1\0", store)?,
        proposal.project_id(),
        0,
        None,
        0,
        Sha256Digest::new([0; 32]),
        fixture.policy.digest(),
        PointerCommandKind::InitializeProductionHarness {
            initial: fixture.baseline,
            policy: fixture.policy.clone(),
            limits: EvolutionLimits::compiled(),
            evidence_artifact: artifacts.initialization,
            evidence_digest: artifacts.initialization_evidence,
        },
    )?;
    let initialized = decide_pointer(None, &genesis)?;
    commit_pointer_transition(owner, &genesis, &initialized)?;
    let prepare = next_pointer_command(
        initialized.state(),
        command(b"peritus/h1/promotion/pointer-command-2/v1\0", store)?,
        event(b"peritus/h1/promotion/pointer-event-2/v1\0", store)?,
        PointerCommandKind::PreparePromotion(proposal.clone()),
    )?;
    let prepared = decide_pointer(Some(initialized.state()), &prepare)?;
    commit_pointer_transition(owner, &prepare, &prepared)?;
    Ok(prepared.state().clone())
}

fn campaign_command(
    prior: Option<&CampaignState>,
    campaign_id: EvolutionCampaignId,
    policy_digest: Sha256Digest,
    kind: CampaignCommandKind,
    step: u8,
    store: StoreId,
) -> Result<CampaignCommand, crate::EvolutionError> {
    let command_domain = format!("peritus/h1/promotion/campaign-command-{step}/v1\0");
    let event_domain = format!("peritus/h1/promotion/campaign-event-{step}/v1\0");
    CampaignCommand::new(
        command(command_domain.as_bytes(), store)?,
        event(event_domain.as_bytes(), store)?,
        campaign_id,
        prior.map_or(0, CampaignState::sequence),
        prior.map(CampaignState::last_event),
        prior.map_or(Sha256Digest::new([0; 32]), CampaignState::state_digest),
        policy_digest,
        kind,
    )
}

pub(super) fn next_campaign_command(
    state: &CampaignState,
    command_id: CommandId,
    event_id: EventId,
    kind: CampaignCommandKind,
) -> Result<CampaignCommand, crate::EvolutionError> {
    CampaignCommand::new(
        command_id,
        event_id,
        state.campaign_id(),
        state.sequence(),
        Some(state.last_event()),
        state.state_digest(),
        state.policy().policy().digest(),
        kind,
    )
}

pub(super) fn next_pointer_command(
    state: &ProductionHarnessState,
    command_id: CommandId,
    event_id: EventId,
    kind: PointerCommandKind,
) -> Result<PointerCommand, crate::EvolutionError> {
    PointerCommand::new(
        command_id,
        event_id,
        state.project_id(),
        state.sequence(),
        Some(state.last_event()),
        state.generation(),
        state.state_digest(),
        state.policy().digest(),
        kind,
    )
}

pub(super) fn finalize_artifacts(
    owner: &ArtifactStore,
    store: StoreId,
) -> Result<QualificationArtifacts, crate::EvolutionError> {
    let initialization = finalize(owner, b"promotion initialization evidence", 1, store)?;
    let baseline = finalize(owner, b"promotion baseline evidence", 2, store)?;
    let diagnosis = finalize(owner, b"promotion diagnosis evidence", 3, store)?;
    let semantic_diff = finalize(owner, b"promotion semantic diff", 4, store)?;
    let evaluation = finalize(owner, b"promotion evaluation report", 5, store)?;
    let evidence_bundle = finalize(owner, b"promotion evidence bundle", 6, store)?;
    Ok(QualificationArtifacts {
        initialization: initialization.0,
        initialization_evidence: initialization.1,
        baseline: baseline.0,
        baseline_evidence: baseline.1,
        diagnosis: diagnosis.0,
        semantic_diff: semantic_diff.0,
        evaluation: evaluation.0,
        evidence_bundle: evidence_bundle.0,
    })
}

fn finalize(
    owner: &ArtifactStore,
    bytes: &[u8],
    step: u8,
    store: StoreId,
) -> Result<(Sha256Digest, Sha256Digest), crate::EvolutionError> {
    let semantic_domain = format!("peritus/h1/promotion/artifact-semantic-{step}/v1\0");
    let event_domain = format!("peritus/h1/promotion/artifact-event-{step}/v1\0");
    let finalized = finalize_evolution_artifact(
        owner,
        bytes,
        digest(semantic_domain.as_bytes(), store),
        event(event_domain.as_bytes(), store)?,
    )?;
    Ok((finalized.artifact_digest().sha256(), finalized.semantic_digest()))
}

#[cfg(test)]
mod tests {
    use peritus_codec::{CodecLimits, decode_message, encode_message};
    use peritus_journal::StoreId;

    use crate::{CampaignCommandKind, EvolutionLimits, wire::CampaignCommandFrame};

    use super::{HarnessFixture, campaign_command};
    use crate::qualification::identity::{digest, nominal};

    #[test]
    fn qualification_campaign_genesis_round_trips_canonical_wire() {
        let store = StoreId::new([0x42; 16]).expect("store");
        let fixture = HarnessFixture::build(store).expect("harness fixture");
        let command = campaign_command(
            None,
            crate::EvolutionCampaignId::new(nominal(b"test-campaign", store)).expect("campaign"),
            fixture.policy.policy().digest(),
            CampaignCommandKind::CreateCampaign {
                project_id: peritus_types::ProjectId::new(nominal(b"test-project", store))
                    .expect("project"),
                baseline: fixture.baseline,
                policy: fixture.policy,
                limits: EvolutionLimits::compiled(),
            },
            1,
            store,
        )
        .expect("genesis command");
        assert_ne!(command.digest(), digest(b"unrelated", store));
        let frame = CampaignCommandFrame::from_command(&command).expect("command frame");
        let bytes = encode_message(&frame, CodecLimits::PRODUCTION).expect("encode command");
        decode_message::<CampaignCommandFrame>(&bytes, CodecLimits::PRODUCTION)
            .expect("decode command");
    }
}
