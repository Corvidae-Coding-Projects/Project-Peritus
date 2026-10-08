//! Atomic ordinary campaign event and checkpoint persistence.

use peritus_codec::{CodecLimits, encode_message};
use peritus_journal::{
    AppendRequest, ArtifactDependency, CommandResolution, CommittedBatch, EventDraft, ExactFrame,
    HeadExpectation, SqliteJournal,
};
use peritus_types::EventSequence;

use crate::{
    CampaignCommand, CampaignCommandKind, CampaignState, CampaignTransition, EvolutionError,
    EvolutionErrorKind, EvolutionOperation, EvolutionRecovery, EvolutionStorageLimits,
    wire::{CampaignCommandFrame, CampaignEventFrame},
};

use super::{
    CAMPAIGN_STATE_NAMESPACE, binding, campaign_aggregate_key, campaign_state_key,
    checkpoint,
    directive::campaign_outbox,
};

/// Atomically appends one accepted campaign event and its complete checkpoint.
///
/// # Errors
/// Rejects transition drift, stale C0 fences, missing artifacts, protocol errors, or journal
/// failures.
pub fn commit_campaign_transition(
    journal: &mut SqliteJournal,
    command: &CampaignCommand,
    transition: &CampaignTransition,
) -> Result<CommittedBatch, EvolutionError> {
    commit_campaign_transition_with_storage(
        journal,
        command,
        transition,
        EvolutionStorageLimits::default(),
    )
}

/// Atomically appends one campaign transition with caller-selected physical checkpoint paging.
///
/// # Errors
/// Rejects transition drift, stale C0 fences, missing artifacts, protocol errors, or journal
/// failures.
pub fn commit_campaign_transition_with_storage(
    journal: &mut SqliteJournal,
    command: &CampaignCommand,
    transition: &CampaignTransition,
    storage: EvolutionStorageLimits,
) -> Result<CommittedBatch, EvolutionError> {
    binding::validate_campaign(command, transition)?;
    let aggregate = campaign_aggregate_key(command.campaign_id())?;
    let state_key = campaign_state_key(command.campaign_id());
    let command_frame = CampaignCommandFrame::from_command(command).map_err(codec)?;
    let command_bytes =
        encode_message(&command_frame, CodecLimits::PRODUCTION).map_err(codec)?;
    let event_bytes = encode_message(
        &CampaignEventFrame::from_event_and_command(transition.event(), &command_frame)
            .map_err(codec)?,
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    let request_digest = peritus_codec::sha256(&command_bytes);
    if let Some(batch) = resolve_existing(
        journal,
        command,
        aggregate,
        &state_key,
        &event_bytes,
        transition.state(),
        request_digest,
    )? {
        return Ok(batch);
    }
    let head = journal.head(aggregate).map_err(journal_error)?;
    let current =
        journal.state_record(CAMPAIGN_STATE_NAMESPACE, &state_key).map_err(journal_error)?;
    validate_current(journal, command, head, current.as_ref())?;
    let event = transition.event();
    let draft = EventDraft::new(
        aggregate,
        EventSequence::new(event.sequence())
            .map_err(|_| binding::binding("zero campaign event"))?,
        event.id(),
        event.previous_event(),
        ExactFrame::new(event_bytes).map_err(journal_error)?,
        event_revision_digest(command, transition.state()),
        Vec::new(),
    )
    .map_err(journal_error)?;
    let installs = checkpoint::campaign_installs(
        &state_key,
        current.as_ref().map(peritus_journal::DurableStateRecord::revision),
        transition.state(),
        storage,
    )?;
    let expectation = head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present);
    let request = AppendRequest::new(
        journal.store_id(),
        command.command_id(),
        request_digest,
        vec![expectation],
        vec![draft],
        installs,
        artifact_dependencies(command.kind()),
        None,
        None,
        campaign_outbox(command)?,
    );
    journal.append(request.plan().map_err(journal_error)?).map_err(journal_error)
}

/// Resolves an exact campaign command receipt before reducer capacity or state fences are read.
///
/// # Errors
/// Rejects a command identity already bound to another request or a receipt for another aggregate.
pub fn resolve_campaign_receipt(
    journal: &SqliteJournal,
    command: &CampaignCommand,
) -> Result<Option<CommittedBatch>, EvolutionError> {
    let aggregate = campaign_aggregate_key(command.campaign_id())?;
    let command_bytes = encode_message(
        &CampaignCommandFrame::from_command(command).map_err(codec)?,
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    match journal
        .resolve_command(command.command_id(), peritus_codec::sha256(&command_bytes))
        .map_err(journal_error)?
    {
        CommandResolution::Committed(batch) => {
            validate_resolved_receipt(
                journal,
                command,
                aggregate,
                &campaign_state_key(command.campaign_id()),
                &batch,
            )?;
            Ok(Some(batch))
        }
        CommandResolution::Conflict { .. } => {
            Err(binding::binding("campaign command identity has another request"))
        }
        CommandResolution::DefinitelyAbsent => Ok(None),
    }
}

fn event_revision_digest(
    command: &CampaignCommand,
    state: &CampaignState,
) -> peritus_types::Sha256Digest {
    match command.kind() {
        CampaignCommandKind::RequestPromotion(proposal) => {
            peritus_evidence::revision_digest(&proposal.candidate().revision())
        }
        _ => state.state_digest(),
    }
}

pub(super) fn artifact_dependencies(kind: &CampaignCommandKind) -> Vec<ArtifactDependency> {
    let mut values = match kind {
        CampaignCommandKind::RecordBaselineEvidence { artifact_digest, .. } => {
            vec![ArtifactDependency::new(*artifact_digest)]
        }
        CampaignCommandKind::SubmitDiagnosis(value) => {
            vec![ArtifactDependency::new(value.artifact_digest())]
        }
        CampaignCommandKind::AdmitChangeManifest(value) => value
            .deltas()
            .iter()
            .flat_map(|delta| {
                [Some(delta.semantic_diff_artifact()), delta.migration_artifact()]
                    .into_iter()
                    .flatten()
            })
            .map(ArtifactDependency::new)
            .collect(),
        CampaignCommandKind::AdmitEvaluation { evidence, .. } => {
            vec![ArtifactDependency::new(evidence.report_artifact())]
        }
        CampaignCommandKind::SupersedeEvaluation(value) => vec![
            ArtifactDependency::new(value.predecessor_report_artifact()),
            ArtifactDependency::new(value.successor().report_artifact()),
        ],
        CampaignCommandKind::RequestPromotion(value) => {
            vec![ArtifactDependency::new(value.evidence_bundle_artifact())]
        }
        CampaignCommandKind::RecordPublication(value) => {
            vec![ArtifactDependency::new(value.artifact_digest())]
        }
        CampaignCommandKind::CreateCampaign { .. }
        | CampaignCommandKind::FreezeCampaign
        | CampaignCommandKind::FreezeCampaignWithMeasurements { .. }
        | CampaignCommandKind::ExpandScope { .. }
        | CampaignCommandKind::AdmitVariant(_)
        | CampaignCommandKind::CompleteAttribution { .. }
        | CampaignCommandKind::RecordSelection(_)
        | CampaignCommandKind::ActivatePromotion { .. }
        | CampaignCommandKind::CancelCampaign { .. }
        | CampaignCommandKind::FailCampaign { .. } => Vec::new(),
    };
    values.sort_unstable();
    values.dedup();
    values
}

pub(super) fn validate_current(
    journal: &SqliteJournal,
    command: &CampaignCommand,
    head: Option<peritus_journal::AggregateHead>,
    current: Option<&peritus_journal::DurableStateRecord>,
) -> Result<(), EvolutionError> {
    if head.is_some() != current.is_some() {
        return Err(recovery("campaign head/checkpoint presence differs"));
    }
    match head {
        None if command.expected_sequence() != 0 => {
            return Err(binding::binding("campaign genesis expects an existing head"));
        }
        Some(observed)
            if observed.sequence().get() != command.expected_sequence()
                || Some(observed.event_id()) != command.expected_head() =>
        {
            return Err(binding::binding("campaign command fence differs from C0 head"));
        }
        _ => {}
    }
    if let Some(record) = current {
        if record.revision() != command.expected_sequence() {
            return Err(recovery("campaign checkpoint revision differs from C0 head"));
        }
        let frame = checkpoint::decode_campaign(journal, record, command.campaign_id())?;
        if frame.campaign_id() != command.campaign_id()
            || frame.sequence() != command.expected_sequence()
            || Some(frame.last_event_id()) != command.expected_head()
            || frame.state_digest() != command.prior_state_digest()
        {
            return Err(binding::binding("campaign command fence differs from durable checkpoint"));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn resolve_existing(
    journal: &SqliteJournal,
    command: &CampaignCommand,
    aggregate: peritus_journal::AggregateKey,
    state_key: &[u8],
    event_bytes: &[u8],
    state: &CampaignState,
    request_digest: peritus_types::Sha256Digest,
) -> Result<Option<CommittedBatch>, EvolutionError> {
    let batch = match journal
        .resolve_command(command.command_id(), request_digest)
        .map_err(journal_error)?
    {
        CommandResolution::Committed(batch) => batch,
        CommandResolution::Conflict { .. } => {
            return Err(binding::binding("campaign command identity has another request"));
        }
        CommandResolution::DefinitelyAbsent => return Ok(None),
    };
    let observed = validate_resolved_receipt(journal, command, aggregate, state_key, &batch)?;
    if batch.records()[0].frame_bytes() != event_bytes {
        return Err(recovery("resolved campaign command differs from its event"));
    }
    if observed.matches_state(state) {
        Ok(Some(batch))
    } else {
        Err(recovery("resolved campaign checkpoint differs from successor"))
    }
}

fn validate_resolved_receipt(
    journal: &SqliteJournal,
    command: &CampaignCommand,
    aggregate: peritus_journal::AggregateKey,
    state_key: &[u8],
    batch: &CommittedBatch,
) -> Result<checkpoint::CampaignCheckpoint, EvolutionError> {
    let successor_sequence = command
        .expected_sequence()
        .checked_add(1)
        .ok_or_else(|| recovery("resolved campaign sequence overflows"))?;
    let [record] = batch.records() else {
        return Err(recovery("campaign receipt belongs to another durable effect"));
    };
    if record.aggregate() != aggregate
        || record.sequence().get() != successor_sequence
        || record.event_id() != command.event_id()
        || record.previous_event_id() != command.expected_head()
    {
        return Err(recovery("resolved campaign command differs from its event identity"));
    }
    let checkpoint = journal
        .state_record_revision(CAMPAIGN_STATE_NAMESPACE, state_key, successor_sequence)
        .map_err(journal_error)?
        .ok_or_else(|| recovery("resolved campaign command has no historical checkpoint"))?;
    let observed = checkpoint::decode_campaign(journal, &checkpoint, command.campaign_id())?;
    if checkpoint.producing_position() != batch.last_position()
        || checkpoint.revision() != successor_sequence
        || observed.campaign_id() != command.campaign_id()
        || observed.sequence() != successor_sequence
        || observed.last_event_id() != command.event_id()
    {
        return Err(recovery(
            "resolved campaign event and historical checkpoint have different origins",
        ));
    }
    Ok(observed)
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "map_err supplies the owned codec failure to this redacting boundary"
)]
pub(super) fn codec(error: impl core::fmt::Display) -> EvolutionError {
    drop(error);
    EvolutionError::new(
        EvolutionErrorKind::Codec,
        EvolutionOperation::Codec,
        EvolutionRecovery::Quarantine,
        "campaign frame violates canonical protocol",
    )
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "map_err supplies the owned journal failure to this typed recovery classifier"
)]
pub(super) fn journal_error(error: peritus_journal::JournalError) -> EvolutionError {
    let recovery = match error.kind() {
        peritus_journal::JournalErrorKind::Busy
        | peritus_journal::JournalErrorKind::IndeterminateCommit => EvolutionRecovery::Retry,
        peritus_journal::JournalErrorKind::StaleHead
        | peritus_journal::JournalErrorKind::StaleAuthorityEpoch
        | peritus_journal::JournalErrorKind::StaleRegistry => EvolutionRecovery::RefreshState,
        _ => EvolutionRecovery::Replay,
    };
    EvolutionError::new(
        EvolutionErrorKind::Journal,
        EvolutionOperation::Commit,
        recovery,
        "C0 rejected or could not commit the campaign transition",
    )
}

pub(super) const fn recovery(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Corruption,
        EvolutionOperation::Recover,
        EvolutionRecovery::Quarantine,
        detail,
    )
}
