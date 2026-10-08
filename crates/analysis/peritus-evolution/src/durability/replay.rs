//! Authenticated, resumable recovery for evolution aggregates.

use peritus_codec::{CodecLimits, decode_message};
use peritus_journal::{
    AggregateHead, CommittedRecord, DurableStateRecord, MAX_GLOBAL_WINDOW_RECORDS,
    SqliteJournal, StoreId,
};
use peritus_types::ProjectId;

use crate::{
    CampaignEvent, CampaignState, DurableActivationHistory, DurableActivationOrigin,
    EvolutionCampaignId, EvolutionError, PendingActivation, PointerCommandKind, PointerEvent,
    PointerEventKind, ProductionHarnessState, apply_pointer_event,
    apply_pointer_event_legacy_eviction,
    wire::{CampaignEventFrame, PointerEventFrame},
};

use super::{
    CAMPAIGN_STATE_NAMESPACE, POINTER_STATE_NAMESPACE, campaign::recovery,
    campaign_aggregate_key, campaign_state_key, checkpoint,
    pointer::pointer_event_revision_digest, pointer_aggregate_key, pointer_state_key,
};

/// Largest physical event window processed by one recovery resumption.
///
/// This is a per-call work quantum. It does not limit the history accepted by recovery.
pub const EVOLUTION_REPLAY_WINDOW_EVENTS: usize = MAX_GLOBAL_WINDOW_RECORDS;

/// Observable position of an authenticated replay cursor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvolutionReplayProgress {
    completed_events: u64,
    total_events: u64,
}

impl EvolutionReplayProgress {
    /// Events already checked and reduced into the cursor state.
    #[must_use]
    pub const fn completed_events(self) -> u64 {
        self.completed_events
    }

    /// Events in the checkpoint/head frontier frozen when recovery started.
    #[must_use]
    pub const fn total_events(self) -> u64 {
        self.total_events
    }
}

/// Decision returned by a replay progress observer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvolutionReplayControl {
    /// Continue replay while the current bounded work quantum remains.
    Continue,
    /// Return the authenticated cursor without discarding completed work.
    Cancel,
}

/// Contiguous checked campaign events and their reconstructed state.
#[derive(Debug)]
pub struct CampaignReplay {
    store_id: StoreId,
    events: Vec<CampaignEvent>,
    state: Option<CampaignState>,
}

impl CampaignReplay {
    /// Durable store identity observed while replaying.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }

    /// Complete ordered semantic campaign events.
    #[must_use]
    pub fn events(&self) -> &[CampaignEvent] {
        &self.events
    }

    /// Reconstructed campaign state, or `None` for an absent aggregate.
    #[must_use]
    pub const fn state(&self) -> Option<&CampaignState> {
        self.state.as_ref()
    }
}

/// Contiguous checked pointer events, reconstructed state, and durable activation origins.
#[derive(Debug)]
pub struct PointerReplay {
    store_id: StoreId,
    events: Vec<PointerEvent>,
    state: Option<ProductionHarnessState>,
    activation_history: DurableActivationHistory,
}

impl PointerReplay {
    /// Durable store identity observed while replaying.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }

    /// Complete ordered semantic pointer events.
    #[must_use]
    pub fn events(&self) -> &[PointerEvent] {
        &self.events
    }

    /// Reconstructed current pointer state, or `None` for an absent aggregate.
    #[must_use]
    pub const fn state(&self) -> Option<&ProductionHarnessState> {
        self.state.as_ref()
    }

    /// Complete activation ledger reconstructed from immutable event/checkpoint origins.
    #[must_use]
    pub const fn activation_history(&self) -> &DurableActivationHistory {
        &self.activation_history
    }
}

/// Resumable campaign recovery state bound to one observed head and checkpoint.
#[derive(Debug)]
pub struct CampaignRecoveryCursor {
    store_id: StoreId,
    campaign_id: EvolutionCampaignId,
    target_head: AggregateHead,
    checkpoint_digest: peritus_types::Sha256Digest,
    checkpoint_position: u64,
    events: Vec<CampaignEvent>,
    state: Option<CampaignState>,
    last_record: Option<CommittedRecord>,
}

impl CampaignRecoveryCursor {
    /// Current checked event count and the frozen target count.
    #[must_use]
    pub fn progress(&self) -> EvolutionReplayProgress {
        EvolutionReplayProgress {
            completed_events: self.state.as_ref().map_or(0, CampaignState::sequence),
            total_events: self.target_head.sequence().get(),
        }
    }
}

/// Outcome of starting or resuming campaign recovery.
#[derive(Debug)]
pub enum CampaignRecoveryStatus {
    /// The work quantum ended and this cursor can be resumed.
    Pending(CampaignRecoveryCursor),
    /// The observer cancelled and this cursor retains all checked work.
    Cancelled(CampaignRecoveryCursor),
    /// The target checkpoint and every event through it were verified.
    Complete(CampaignReplay),
}

/// Resumable pointer recovery state bound to one observed head and checkpoint.
#[derive(Debug)]
pub struct PointerRecoveryCursor {
    store_id: StoreId,
    project_id: ProjectId,
    target_head: AggregateHead,
    checkpoint_digest: peritus_types::Sha256Digest,
    checkpoint_position: u64,
    events: Vec<PointerEvent>,
    state: Option<ProductionHarnessState>,
    last_record: Option<CommittedRecord>,
    activation_history: DurableActivationHistory,
}

impl PointerRecoveryCursor {
    /// Current checked event count and the frozen target count.
    #[must_use]
    pub fn progress(&self) -> EvolutionReplayProgress {
        EvolutionReplayProgress {
            completed_events: self.state.as_ref().map_or(0, ProductionHarnessState::sequence),
            total_events: self.target_head.sequence().get(),
        }
    }
}

/// Outcome of starting or resuming pointer recovery.
#[derive(Debug)]
pub enum PointerRecoveryStatus {
    /// The work quantum ended and this cursor can be resumed.
    Pending(PointerRecoveryCursor),
    /// The observer cancelled and this cursor retains all checked work.
    Cancelled(PointerRecoveryCursor),
    /// The target checkpoint and every event through it were verified.
    Complete(PointerReplay),
}

/// Starts campaign recovery and authenticates the frozen checkpoint/head frontier.
///
/// # Errors
/// Rejects journal failures, malformed checkpoint pages, or a checkpoint that differs from C0.
pub fn start_campaign_recovery(
    journal: &SqliteJournal,
    campaign_id: EvolutionCampaignId,
) -> Result<CampaignRecoveryStatus, EvolutionError> {
    let aggregate = campaign_aggregate_key(campaign_id)?;
    let state_key = campaign_state_key(campaign_id);
    let head = journal.head(aggregate).map_err(journal_error)?;
    let record = journal
        .state_record(CAMPAIGN_STATE_NAMESPACE, &state_key)
        .map_err(journal_error)?;
    if head.is_some() != record.is_some() {
        return Err(recovery("campaign events/checkpoint presence differs"));
    }
    let (Some(head), Some(record)) = (head, record) else {
        return Ok(CampaignRecoveryStatus::Complete(CampaignReplay {
            store_id: journal.store_id(),
            events: Vec::new(),
            state: None,
        }));
    };
    let observed = checkpoint::decode_campaign(journal, &record, campaign_id)?.into_state();
    if record.revision() != head.sequence().get()
        || observed.campaign_id() != campaign_id
        || observed.sequence() != head.sequence().get()
        || observed.last_event() != head.event_id()
    {
        return Err(recovery("campaign checkpoint differs from its aggregate head"));
    }
    authenticate_target_record(journal, aggregate, &head, &record)?;
    Ok(CampaignRecoveryStatus::Pending(CampaignRecoveryCursor {
        store_id: journal.store_id(),
        campaign_id,
        target_head: head,
        checkpoint_digest: record.digest(),
        checkpoint_position: record.producing_position(),
        events: Vec::new(),
        state: None,
        last_record: None,
    }))
}

/// Resumes campaign recovery for at most one bounded event window.
///
/// The observer is called before work and after every checked event. Returning
/// [`EvolutionReplayControl::Cancel`] yields a resumable cursor.
///
/// # Errors
/// Rejects a zero work quantum, journal drift, malformed events, or reduction/checkpoint mismatch.
pub fn resume_campaign_recovery(
    journal: &SqliteJournal,
    mut cursor: CampaignRecoveryCursor,
    maximum_events: usize,
    mut control: impl FnMut(EvolutionReplayProgress) -> EvolutionReplayControl,
) -> Result<CampaignRecoveryStatus, EvolutionError> {
    if maximum_events == 0 {
        return Err(recovery("campaign recovery work quantum is zero"));
    }
    authenticate_campaign_cursor(journal, &cursor)?;
    if control(cursor.progress()) == EvolutionReplayControl::Cancel {
        return Ok(CampaignRecoveryStatus::Cancelled(cursor));
    }
    let progress = cursor.progress();
    if progress.completed_events == progress.total_events {
        return finish_campaign_recovery(journal, cursor).map(CampaignRecoveryStatus::Complete);
    }
    let remaining = progress.total_events - progress.completed_events;
    let maximum = maximum_events.min(MAX_GLOBAL_WINDOW_RECORDS);
    let limit = usize::try_from(remaining).unwrap_or(usize::MAX).min(maximum);
    let aggregate = campaign_aggregate_key(cursor.campaign_id)?;
    let records = journal
        .aggregate_events_after(aggregate, progress.completed_events, limit)
        .map_err(journal_error)?;
    if records.is_empty() {
        return Err(recovery("campaign recovery made no progress before its target head"));
    }
    cursor
        .events
        .try_reserve(records.len())
        .map_err(|_| recovery("campaign recovery cannot reserve its bounded event window"))?;
    for record in records {
        let frame = decode_message::<CampaignEventFrame>(
            record.frame_bytes(),
            CodecLimits::PRODUCTION,
        )
        .map_err(codec)?;
        let (event, reconstructed) = frame.check_transition(cursor.state.as_ref())?;
        validate_campaign_record(&record, &event, cursor.campaign_id, cursor.progress())?;
        cursor.state = Some(reconstructed);
        cursor.events.push(event);
        cursor.last_record = Some(record);
        if control(cursor.progress()) == EvolutionReplayControl::Cancel {
            return Ok(CampaignRecoveryStatus::Cancelled(cursor));
        }
    }
    if cursor.progress().completed_events == cursor.progress().total_events {
        finish_campaign_recovery(journal, cursor).map(CampaignRecoveryStatus::Complete)
    } else {
        Ok(CampaignRecoveryStatus::Pending(cursor))
    }
}

/// Starts pointer recovery and authenticates the frozen checkpoint/head frontier.
///
/// # Errors
/// Rejects journal failures, malformed checkpoint pages, or a checkpoint that differs from C0.
pub fn start_pointer_recovery(
    journal: &SqliteJournal,
    project_id: ProjectId,
) -> Result<PointerRecoveryStatus, EvolutionError> {
    let aggregate = pointer_aggregate_key(project_id)?;
    let state_key = pointer_state_key(project_id);
    let head = journal.head(aggregate).map_err(journal_error)?;
    let record = journal
        .state_record(POINTER_STATE_NAMESPACE, &state_key)
        .map_err(journal_error)?;
    if head.is_some() != record.is_some() {
        return Err(recovery("pointer events/checkpoint presence differs"));
    }
    let (Some(head), Some(record)) = (head, record) else {
        return Ok(PointerRecoveryStatus::Complete(PointerReplay {
            store_id: journal.store_id(),
            events: Vec::new(),
            state: None,
            activation_history: DurableActivationHistory::empty(journal.store_id(), project_id),
        }));
    };
    let observed = checkpoint::decode_pointer(journal, &record, project_id)?.into_state();
    if record.revision() != head.sequence().get()
        || observed.project_id() != project_id
        || observed.sequence() != head.sequence().get()
        || observed.last_event() != head.event_id()
    {
        return Err(recovery("pointer checkpoint differs from its aggregate head"));
    }
    authenticate_target_record(journal, aggregate, &head, &record)?;
    Ok(PointerRecoveryStatus::Pending(PointerRecoveryCursor {
        store_id: journal.store_id(),
        project_id,
        target_head: head,
        checkpoint_digest: record.digest(),
        checkpoint_position: record.producing_position(),
        events: Vec::new(),
        state: None,
        last_record: None,
        activation_history: DurableActivationHistory::empty(journal.store_id(), project_id),
    }))
}

/// Resumes pointer recovery for at most one bounded event window.
///
/// The observer is called before work and after every checked event. Returning
/// [`EvolutionReplayControl::Cancel`] yields a resumable cursor.
///
/// # Errors
/// Rejects a zero work quantum, journal drift, malformed events, invalid rollback provenance, or
/// reduction/checkpoint mismatch.
pub fn resume_pointer_recovery(
    journal: &SqliteJournal,
    mut cursor: PointerRecoveryCursor,
    maximum_events: usize,
    mut control: impl FnMut(EvolutionReplayProgress) -> EvolutionReplayControl,
) -> Result<PointerRecoveryStatus, EvolutionError> {
    if maximum_events == 0 {
        return Err(recovery("pointer recovery work quantum is zero"));
    }
    authenticate_pointer_cursor(journal, &cursor)?;
    if control(cursor.progress()) == EvolutionReplayControl::Cancel {
        return Ok(PointerRecoveryStatus::Cancelled(cursor));
    }
    let progress = cursor.progress();
    if progress.completed_events == progress.total_events {
        return finish_pointer_recovery(journal, cursor).map(PointerRecoveryStatus::Complete);
    }
    let remaining = progress.total_events - progress.completed_events;
    let maximum = maximum_events.min(MAX_GLOBAL_WINDOW_RECORDS);
    let limit = usize::try_from(remaining).unwrap_or(usize::MAX).min(maximum);
    let aggregate = pointer_aggregate_key(cursor.project_id)?;
    let records = journal
        .aggregate_events_after(aggregate, progress.completed_events, limit)
        .map_err(journal_error)?;
    if records.is_empty() {
        return Err(recovery("pointer recovery made no progress before its target head"));
    }
    cursor
        .events
        .try_reserve(records.len())
        .map_err(|_| recovery("pointer recovery cannot reserve its bounded event window"))?;
    let state_key = pointer_state_key(cursor.project_id);
    for record in records {
        let frame = decode_message::<PointerEventFrame>(
            record.frame_bytes(),
            CodecLimits::PRODUCTION,
        )
        .map_err(codec)?;
        let event = frame.into_event()?;
        validate_pointer_record(&record, &event, cursor.project_id, cursor.progress())?;
        let historical = journal
            .state_record_revision(POINTER_STATE_NAMESPACE, &state_key, event.sequence())
            .map_err(journal_error)?
            .ok_or_else(|| recovery("pointer event has no immutable checkpoint revision"))?;
        let observed =
            checkpoint::decode_pointer(journal, &historical, cursor.project_id)?.into_state();
        validate_pointer_checkpoint(journal, &record, &event, &historical, &observed)?;
        validate_durable_rollback_event(
            &cursor.activation_history,
            cursor.state.as_ref(),
            &event,
        )?;

        let reconstructed = match apply_pointer_event(cursor.state.as_ref(), &event) {
            Ok(candidate) if candidate == observed => candidate,
            _ if legacy_eviction_shape(cursor.state.as_ref(), &event, &observed) => {
                apply_pointer_event_legacy_eviction(cursor.state.as_ref(), &event)?
            }
            _ => return Err(recovery("pointer checkpoint differs from pure event replay")),
        };
        if reconstructed != observed {
            return Err(recovery("legacy pointer checkpoint differs from exact eviction replay"));
        }

        if event.successor_generation() != event.prior_generation() {
            let activation = observed
                .history()
                .last()
                .cloned()
                .ok_or_else(|| recovery("activation event checkpoint has no activation record"))?;
            let origin = DurableActivationOrigin::from_committed(
                journal.store_id(),
                cursor.project_id,
                activation,
                event.sequence(),
                event.id(),
                event.command_id(),
                record.global_position(),
                record.previous_event_hash(),
                record.event_hash(),
                record.frame_digest(),
                record.revision_digest(),
                event.successor_state_digest(),
                historical.digest(),
                historical.producing_position(),
            )?;
            cursor.activation_history.push(origin)?;
        }
        if !cursor.activation_history.matches_state(&observed) {
            return Err(recovery(
                "pointer checkpoint cache is not a suffix of durable activation history",
            ));
        }
        cursor.state = Some(observed);
        cursor.events.push(event);
        cursor.last_record = Some(record);
        if control(cursor.progress()) == EvolutionReplayControl::Cancel {
            return Ok(PointerRecoveryStatus::Cancelled(cursor));
        }
    }
    if cursor.progress().completed_events == cursor.progress().total_events {
        finish_pointer_recovery(journal, cursor).map(PointerRecoveryStatus::Complete)
    } else {
        Ok(PointerRecoveryStatus::Pending(cursor))
    }
}

/// Recovers a complete campaign through repeated bounded resumptions.
///
/// # Errors
/// Rejects any error reported by start or resume recovery.
pub fn recover_campaign(
    journal: &SqliteJournal,
    campaign_id: EvolutionCampaignId,
) -> Result<CampaignReplay, EvolutionError> {
    let mut status = start_campaign_recovery(journal, campaign_id)?;
    loop {
        status = match status {
            CampaignRecoveryStatus::Complete(replay) => return Ok(replay),
            CampaignRecoveryStatus::Pending(cursor)
            | CampaignRecoveryStatus::Cancelled(cursor) => resume_campaign_recovery(
                journal,
                cursor,
                MAX_GLOBAL_WINDOW_RECORDS,
                |_| EvolutionReplayControl::Continue,
            )?,
        };
    }
}

/// Recovers a complete pointer through repeated bounded resumptions.
///
/// # Errors
/// Rejects any error reported by start or resume recovery.
pub fn recover_pointer(
    journal: &SqliteJournal,
    project_id: ProjectId,
) -> Result<PointerReplay, EvolutionError> {
    let mut status = start_pointer_recovery(journal, project_id)?;
    loop {
        status = match status {
            PointerRecoveryStatus::Complete(replay) => return Ok(replay),
            PointerRecoveryStatus::Pending(cursor)
            | PointerRecoveryStatus::Cancelled(cursor) => resume_pointer_recovery(
                journal,
                cursor,
                MAX_GLOBAL_WINDOW_RECORDS,
                |_| EvolutionReplayControl::Continue,
            )?,
        };
    }
}

fn finish_campaign_recovery(
    journal: &SqliteJournal,
    cursor: CampaignRecoveryCursor,
) -> Result<CampaignReplay, EvolutionError> {
    let record = authenticate_campaign_cursor(journal, &cursor)?;
    let observed = checkpoint::decode_campaign(journal, &record, cursor.campaign_id)?.into_state();
    let reconstructed = cursor
        .state
        .as_ref()
        .ok_or_else(|| recovery("campaign checkpoint exists without reconstructed state"))?;
    let last = cursor
        .last_record
        .as_ref()
        .ok_or_else(|| recovery("campaign checkpoint has no producing event"))?;
    if record.revision() != reconstructed.sequence()
        || !checkpoint_producer_matches(journal, &record, last)?
        || observed != *reconstructed
    {
        return Err(recovery("campaign checkpoint differs from replay"));
    }
    Ok(CampaignReplay {
        store_id: cursor.store_id,
        events: cursor.events,
        state: cursor.state,
    })
}

fn finish_pointer_recovery(
    journal: &SqliteJournal,
    cursor: PointerRecoveryCursor,
) -> Result<PointerReplay, EvolutionError> {
    let record = authenticate_pointer_cursor(journal, &cursor)?;
    let observed = checkpoint::decode_pointer(journal, &record, cursor.project_id)?.into_state();
    let reconstructed = cursor
        .state
        .as_ref()
        .ok_or_else(|| recovery("pointer checkpoint exists without reconstructed state"))?;
    let last = cursor
        .last_record
        .as_ref()
        .ok_or_else(|| recovery("pointer checkpoint has no producing event"))?;
    let historical = journal
        .state_record_revision(
            POINTER_STATE_NAMESPACE,
            &pointer_state_key(cursor.project_id),
            reconstructed.sequence(),
        )
        .map_err(journal_error)?
        .ok_or_else(|| recovery("current pointer has no immutable checkpoint revision"))?;
    if record.revision() != reconstructed.sequence()
        || !checkpoint_producer_matches(journal, &record, last)?
        || record.digest() != historical.digest()
        || record.producing_position() != historical.producing_position()
        || observed != *reconstructed
        || !cursor.activation_history.matches_state(reconstructed)
    {
        return Err(recovery("pointer checkpoint differs from replay"));
    }
    Ok(PointerReplay {
        store_id: cursor.store_id,
        events: cursor.events,
        state: cursor.state,
        activation_history: cursor.activation_history,
    })
}

fn authenticate_campaign_cursor(
    journal: &SqliteJournal,
    cursor: &CampaignRecoveryCursor,
) -> Result<DurableStateRecord, EvolutionError> {
    if cursor.store_id != journal.store_id() {
        return Err(recovery("campaign recovery cursor belongs to another durable store"));
    }
    validate_cursor_progress(
        cursor.progress(),
        cursor.events.len(),
        cursor.last_record.as_ref(),
    )?;
    if let Some(state) = cursor.state.as_ref() {
        let last_event = cursor.events.last().ok_or_else(|| {
            recovery("campaign recovery state exists without a checked semantic event")
        })?;
        if state.campaign_id() != cursor.campaign_id
            || state.last_event() != last_event.id()
            || state.state_digest() != last_event.successor_state_digest()
        {
            return Err(recovery("campaign recovery cursor state is internally inconsistent"));
        }
    }
    authenticate_cursor_target(
        journal,
        campaign_aggregate_key(cursor.campaign_id)?,
        CAMPAIGN_STATE_NAMESPACE,
        &campaign_state_key(cursor.campaign_id),
        &cursor.target_head,
        cursor.checkpoint_digest,
        cursor.checkpoint_position,
    )
}

fn authenticate_pointer_cursor(
    journal: &SqliteJournal,
    cursor: &PointerRecoveryCursor,
) -> Result<DurableStateRecord, EvolutionError> {
    if cursor.store_id != journal.store_id()
        || cursor.activation_history.store_id() != cursor.store_id
        || cursor.activation_history.project_id() != cursor.project_id
    {
        return Err(recovery("pointer recovery cursor belongs to another durable pointer"));
    }
    validate_cursor_progress(
        cursor.progress(),
        cursor.events.len(),
        cursor.last_record.as_ref(),
    )?;
    if let Some(state) = cursor.state.as_ref() {
        let last_event = cursor.events.last().ok_or_else(|| {
            recovery("pointer recovery state exists without a checked semantic event")
        })?;
        if state.project_id() != cursor.project_id
            || state.last_event() != last_event.id()
            || state.state_digest() != last_event.successor_state_digest()
            || !cursor.activation_history.matches_state(state)
        {
            return Err(recovery("pointer recovery cursor state is internally inconsistent"));
        }
    }
    authenticate_cursor_target(
        journal,
        pointer_aggregate_key(cursor.project_id)?,
        POINTER_STATE_NAMESPACE,
        &pointer_state_key(cursor.project_id),
        &cursor.target_head,
        cursor.checkpoint_digest,
        cursor.checkpoint_position,
    )
}

fn validate_cursor_progress(
    progress: EvolutionReplayProgress,
    event_count: usize,
    last_record: Option<&CommittedRecord>,
) -> Result<(), EvolutionError> {
    if progress.completed_events > progress.total_events
        || u64::try_from(event_count) != Ok(progress.completed_events)
        || (progress.completed_events == 0) != last_record.is_none()
        || last_record.is_some_and(|record| {
            record.sequence().get() != progress.completed_events
        })
    {
        return Err(recovery("recovery cursor progress is internally inconsistent"));
    }
    Ok(())
}

#[allow(
    clippy::too_many_arguments,
    reason = "cursor authentication keeps every frozen durable identity explicit"
)]
fn authenticate_cursor_target(
    journal: &SqliteJournal,
    aggregate: peritus_journal::AggregateKey,
    namespace: u16,
    state_key: &[u8],
    target_head: &AggregateHead,
    checkpoint_digest: peritus_types::Sha256Digest,
    checkpoint_position: u64,
) -> Result<DurableStateRecord, EvolutionError> {
    let head = journal.head(aggregate).map_err(journal_error)?;
    let record = journal
        .state_record(namespace, state_key)
        .map_err(journal_error)?;
    let (Some(head), Some(record)) = (head, record) else {
        return Err(recovery("recovery cursor target disappeared from C0"));
    };
    if &head != target_head
        || record.revision() != target_head.sequence().get()
        || record.digest() != checkpoint_digest
        || record.producing_position() != checkpoint_position
    {
        return Err(recovery("recovery cursor target changed and must be restarted"));
    }
    Ok(record)
}

fn authenticate_target_record(
    journal: &SqliteJournal,
    aggregate: peritus_journal::AggregateKey,
    head: &AggregateHead,
    checkpoint: &DurableStateRecord,
) -> Result<(), EvolutionError> {
    let after = head
        .sequence()
        .get()
        .checked_sub(1)
        .ok_or_else(|| recovery("aggregate head sequence is zero"))?;
    let records = journal
        .aggregate_events_after(aggregate, after, 1)
        .map_err(journal_error)?;
    let [record] = records.as_slice() else {
        return Err(recovery("aggregate head event is absent from C0"));
    };
    if record.sequence() != head.sequence()
        || record.event_id() != head.event_id()
        || record.event_hash() != head.event_hash()
        || !checkpoint_producer_matches(journal, checkpoint, record)?
    {
        return Err(recovery("checkpoint is not bound to its exact aggregate head event"));
    }
    Ok(())
}

fn validate_campaign_record(
    record: &CommittedRecord,
    event: &CampaignEvent,
    campaign_id: EvolutionCampaignId,
    prior: EvolutionReplayProgress,
) -> Result<(), EvolutionError> {
    let sequence = prior
        .completed_events
        .checked_add(1)
        .ok_or_else(|| recovery("campaign recovery sequence overflowed"))?;
    if sequence > prior.total_events
        || event.campaign_id() != campaign_id
        || event.sequence() != sequence
        || record.sequence().get() != sequence
        || record.event_id() != event.id()
        || record.command_id() != event.command_id()
        || record.previous_event_id() != event.previous_event()
    {
        return Err(recovery("campaign event differs from its immutable C0 record"));
    }
    Ok(())
}

fn validate_pointer_record(
    record: &CommittedRecord,
    event: &PointerEvent,
    project_id: ProjectId,
    prior: EvolutionReplayProgress,
) -> Result<(), EvolutionError> {
    let sequence = prior
        .completed_events
        .checked_add(1)
        .ok_or_else(|| recovery("pointer recovery sequence overflowed"))?;
    if sequence > prior.total_events
        || event.project_id() != project_id
        || event.sequence() != sequence
        || record.sequence().get() != sequence
        || record.event_id() != event.id()
        || record.command_id() != event.command_id()
        || record.previous_event_id() != event.previous_event()
    {
        return Err(recovery("pointer event differs from its immutable C0 record"));
    }
    Ok(())
}

fn validate_pointer_checkpoint(
    journal: &SqliteJournal,
    record: &CommittedRecord,
    event: &PointerEvent,
    checkpoint: &DurableStateRecord,
    observed: &ProductionHarnessState,
) -> Result<(), EvolutionError> {
    if checkpoint.revision() != event.sequence()
        || !checkpoint_producer_matches(journal, checkpoint, record)?
        || observed.project_id() != event.project_id()
        || observed.sequence() != event.sequence()
        || observed.last_event() != event.id()
        || observed.generation() != event.successor_generation()
        || observed.policy().digest() != event.policy_digest()
        || observed.state_digest() != event.successor_state_digest()
        || record.revision_digest() != pointer_event_revision_digest(observed)
    {
        return Err(recovery(
            "pointer event, committed origin, and historical checkpoint differ",
        ));
    }
    Ok(())
}

fn validate_durable_rollback_event(
    history: &DurableActivationHistory,
    prior: Option<&ProductionHarnessState>,
    event: &PointerEvent,
) -> Result<(), EvolutionError> {
    let PointerEventKind::Accepted(kind) = event.kind();
    let valid = match kind {
        PointerCommandKind::PrepareRollback(proposal) => history.validates_proposal(proposal),
        PointerCommandKind::ActivateRollback { rollback_id, .. } => {
            matches!(
                prior.and_then(ProductionHarnessState::pending),
                Some(PendingActivation::Rollback(proposal))
                    if proposal.id() == *rollback_id && history.validates_proposal(proposal)
            )
        }
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err(recovery(
            "rollback event target has no matching immutable activation origin",
        ))
    }
}

fn legacy_eviction_shape(
    prior: Option<&ProductionHarnessState>,
    event: &PointerEvent,
    observed: &ProductionHarnessState,
) -> bool {
    let Some(prior) = prior else {
        return false;
    };
    let Some(limit) = prior.limits().activation_history_limit().map(usize::from) else {
        return false;
    };
    let PointerEventKind::Accepted(kind) = event.kind();
    let activation = matches!(
        kind,
        PointerCommandKind::ActivatePromotion { .. }
            | PointerCommandKind::ActivateRollback { .. }
    );
    if !activation
        || prior.history().len() != limit
        || observed.history().len() != limit
        || prior.generation().checked_add(1) != Some(observed.generation())
        || event.successor_generation() != observed.generation()
    {
        return false;
    }
    let retained = &prior.history()[1..];
    observed.history().get(..retained.len()) == Some(retained)
}

fn checkpoint_producer_matches(
    journal: &SqliteJournal,
    checkpoint: &DurableStateRecord,
    record: &CommittedRecord,
) -> Result<bool, EvolutionError> {
    let batch = journal
        .command_batch(record.command_id())
        .map_err(journal_error)?;
    Ok(batch.is_some_and(|batch| {
        batch.last_position() == checkpoint.producing_position()
            && batch.records().iter().any(|candidate| candidate == record)
    }))
}

fn codec(_: impl core::fmt::Display) -> EvolutionError {
    recovery("evolution journal frame violates canonical protocol")
}

fn journal_error(_: impl core::fmt::Display) -> EvolutionError {
    recovery("C0 failed while loading evolution replay")
}
