//! Pure and durable shadow-generation rebuild preparation.

use crate::catalog::plan_repair;
use crate::replay::ReplayCheckpoint;
use crate::sqlite::{StoredProgress, WorkPhase};
use crate::{
    ActiveGeneration, CatalogGeneration, Checkpoint, Projection, ProjectionError,
    ProjectionErrorKind, ProjectionState, RecoveryClass, RepairAction, ReplayOutput,
    replay_from_genesis,
};
use peritus_codec::sha256;
use peritus_journal::{
    IntegrityExport, JournalCancellation, SqliteJournal, StoreId, MAX_GLOBAL_WINDOW_RECORDS,
};
use peritus_types::Sha256Digest;

/// Fully checked immutable candidate ready for a transactional generation install.
#[derive(Debug)]
pub struct RebuildCandidate<S> {
    owner_store_id: StoreId,
    output: ReplayOutput<S>,
}

impl<S> RebuildCandidate<S> {
    /// Returns the durable journal owner whose verified history produced this candidate.
    #[must_use]
    pub const fn owner_store_id(&self) -> StoreId {
        self.owner_store_id
    }

    /// Borrows the completed state for caller-side invariant inspection.
    #[must_use]
    pub const fn state(&self) -> &S {
        self.output.state()
    }

    /// Borrows the exact payload to persist.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        self.output.payload()
    }

    /// Borrows the exact checkpoint binding.
    #[must_use]
    pub const fn checkpoint(&self) -> &Checkpoint {
        self.output.checkpoint()
    }

    /// Returns the independent fold-invariant checksum.
    #[must_use]
    pub const fn invariant_digest(&self) -> Sha256Digest {
        self.output.invariant_digest()
    }

    /// Borrows the canonical aggregate frontier persisted with this generation.
    #[must_use]
    pub fn frontier_payload(&self) -> &[u8] {
        self.output.frontier_payload()
    }

    /// Returns the canonical aggregate-frontier digest.
    #[must_use]
    pub const fn frontier_digest(&self) -> Sha256Digest {
        self.output.frontier_digest()
    }

    /// Returns the number of records folded.
    #[must_use]
    pub const fn record_count(&self) -> u64 {
        self.output.record_count()
    }
}

/// Builds and verifies a shadow candidate entirely in memory.
///
/// # Errors
///
/// Returns any checked replay or projection invariant failure.
pub fn rebuild_from_genesis<P: Projection>(
    projection: &P,
    export: &IntegrityExport,
) -> Result<RebuildCandidate<P::State>, ProjectionError> {
    replay_from_genesis(projection, export).map(|output| RebuildCandidate {
        owner_store_id: export.report().store_id(),
        output,
    })
}

/// Restores accepted durable projection work and applies only checked immutable journal suffixes.
///
/// The durable owner is the journal store identity plus projection identity and source generation.
/// The checkpoint is a derived summary and never claims to restore a run, provider, process, or
/// application-native context. The prior active generation remains published until the completed
/// state and aggregate frontier are atomically installed together.
///
/// # Errors
///
/// Returns checked journal, typed state, cancellation, contention, or atomic-install failures.
pub fn resume_or_rebuild<P: Projection>(
    store: &mut crate::ProjectionStore,
    journal: &mut SqliteJournal,
    projection: &P,
    cancellation: &JournalCancellation,
) -> Result<CatalogGeneration, ProjectionError> {
    ensure_live(cancellation)?;
    let owner = journal.store_id();
    let initial_tip = tip_export(journal)?;
    verify_owner(owner, &initial_tip)?;
    let active = store.load_active(projection.schema())?;
    let mut action = plan_repair(
        active.as_ref(),
        projection.schema(),
        initial_tip.report().last_position(),
        initial_tip.report().journal_head_digest(),
    );
    if let RepairAction::Reuse(generation) = action {
        match store.confirm_current(
            projection.schema(),
            generation,
            owner,
            initial_tip.report().last_position(),
            initial_tip.report().journal_head_digest(),
        ) {
            Ok(confirmed) => return Ok(confirmed),
            Err(error) if error.kind() == ProjectionErrorKind::JournalAdvanced => {
                action = RepairAction::CatchUpFromCheckpoint(generation);
            }
            Err(error) => return Err(error),
        }
    }
    let mut source_generation = active.as_ref().map(ActiveGeneration::generation);
    let mut replay = restore_progress(
        store,
        projection,
        owner,
        source_generation,
        initial_tip.report().last_position(),
        initial_tip.report().journal_head_digest(),
    )?
    .or_else(|| {
        if matches!(action, RepairAction::CatchUpFromCheckpoint(_)) {
            active.as_ref().and_then(restore_active::<P>)
        } else {
            None
        }
    })
    .unwrap_or_else(|| ReplayCheckpoint::genesis(projection.genesis()));
    persist_replay(
        store,
        projection,
        owner,
        source_generation,
        WorkPhase::Folding,
        None,
        &replay,
    )?;

    loop {
        ensure_live(cancellation)?;
        let cursor = replay.last_position();
        let window = journal
            .global_events_after(cursor, MAX_GLOBAL_WINDOW_RECORDS)
            .map_err(|error| ProjectionError::journal("read verified projection suffix", error))?;
        if window.has_retention_gap_after(cursor) {
            return Err(ProjectionError::new(
                ProjectionErrorKind::PositionGap,
                RecoveryClass::RepairJournal,
                "resume projection rebuild",
                "accepted projection frontier is older than retained journal history",
            ));
        }
        if window.latest() < cursor {
            return Err(ProjectionError::new(
                ProjectionErrorKind::StaleCheckpoint,
                RecoveryClass::RepairJournal,
                "resume projection rebuild",
                "authoritative journal position moved behind accepted projection work",
            ));
        }
        if window.records().is_empty() {
            let tip = tip_export(journal)?;
            verify_owner(owner, &tip)?;
            if tip.report().last_position() > cursor {
                continue;
            }
            if tip.report().last_position() < cursor {
                return Err(ProjectionError::new(
                    ProjectionErrorKind::StaleCheckpoint,
                    RecoveryClass::RepairJournal,
                    "finish projection rebuild",
                    "authoritative journal position moved behind accepted projection work",
                ));
            }
            let output =
                replay.finish(projection.schema(), tip.report(), tip.heads())?;
            let candidate = RebuildCandidate { owner_store_id: owner, output };
            persist_candidate(store, projection, source_generation, &candidate)?;
            ensure_live(cancellation)?;
            let confirmed = tip_export(journal)?;
            verify_owner(owner, &confirmed)?;
            if confirmed.report().last_position() == candidate.checkpoint().last_position()
                && confirmed.report().journal_head_digest()
                    == candidate.checkpoint().journal_head_digest()
            {
                match store.install_shadow(&candidate, source_generation) {
                    Ok(generation) => return Ok(generation),
                    Err(error)
                        if matches!(
                            error.kind(),
                            ProjectionErrorKind::JournalAdvanced | ProjectionErrorKind::Conflict
                        ) =>
                    {
                        replay = resume_after_install_race(
                            store,
                            projection,
                            owner,
                            &mut source_generation,
                            &candidate,
                        )?;
                        continue;
                    }
                    Err(error) => return Err(error),
                }
            }
            if confirmed.report().last_position() > candidate.checkpoint().last_position() {
                replay = ReplayCheckpoint::restore(
                    candidate.payload(),
                    candidate.invariant_digest(),
                    candidate.frontier_payload(),
                    candidate.checkpoint().last_position(),
                    candidate.record_count(),
                )?;
                persist_replay(
                    store,
                    projection,
                    owner,
                    source_generation,
                    WorkPhase::Folding,
                    None,
                    &replay,
                )?;
                continue;
            }
            return Err(ProjectionError::new(
                ProjectionErrorKind::StaleCheckpoint,
                RecoveryClass::RepairJournal,
                "confirm projection rebuild",
                "journal head changed without an append-only position advance",
            ));
        }

        for record in window.records() {
            ensure_live(cancellation)?;
            let batch = journal
                .integrity_export_for_retained_event(record.global_position())
                .map_err(|error| {
                    ProjectionError::journal("verify projection suffix command batch", error)
                })?;
            verify_owner(owner, &batch)?;
            if !batch.records().iter().any(|candidate| candidate == record) {
                return Err(ProjectionError::new(
                    ProjectionErrorKind::StaleCheckpoint,
                    RecoveryClass::RepairJournal,
                    "verify projection suffix command batch",
                    "scoped integrity export does not contain the exact suffix record",
                ));
            }
            replay.apply(projection, record)?;
            if batch
                .records()
                .last()
                .is_some_and(|candidate| candidate.global_position() == record.global_position())
            {
                replay.apply_supplement(projection, &batch)?;
            }
        }
        persist_replay(
            store,
            projection,
            owner,
            source_generation,
            WorkPhase::Folding,
            None,
            &replay,
        )?;
    }
}

fn restore_active<P: Projection>(
    active: &ActiveGeneration,
) -> Option<ReplayCheckpoint<P::State>> {
    if !active.payload_is_valid() || !active.frontier_is_valid() {
        return None;
    }
    ReplayCheckpoint::restore(
        active.payload(),
        active.invariant_digest(),
        active.frontier_payload()?,
        active.checkpoint().last_position(),
        active.record_count(),
    )
    .ok()
}

fn restore_progress<P: Projection>(
    store: &crate::ProjectionStore,
    projection: &P,
    owner: StoreId,
    source_generation: Option<CatalogGeneration>,
    current_position: u64,
    current_head: Sha256Digest,
) -> Result<Option<ReplayCheckpoint<P::State>>, ProjectionError> {
    let Some(progress) = store.load_progress(projection.schema())? else {
        return Ok(None);
    };
    if progress.schema_digest != projection.schema().digest()
        || progress.owner_store_id != owner
        || progress.source_generation != source_generation
        || progress.cursor_position > current_position
        || (matches!(progress.phase, WorkPhase::Ready)
            && progress.cursor_position == current_position
            && progress.journal_head_digest != Some(current_head))
    {
        return Ok(None);
    }
    Ok(ReplayCheckpoint::restore(
        &progress.payload,
        progress.invariant_digest,
        &progress.frontier,
        progress.cursor_position,
        progress.record_count,
    )
    .ok())
}

fn persist_replay<P: Projection>(
    store: &crate::ProjectionStore,
    projection: &P,
    owner: StoreId,
    source_generation: Option<CatalogGeneration>,
    phase: WorkPhase,
    journal_head_digest: Option<Sha256Digest>,
    replay: &ReplayCheckpoint<P::State>,
) -> Result<(), ProjectionError> {
    replay.state().validate()?;
    let payload = replay.state().encode();
    let frontier = replay.frontier_payload();
    let progress = StoredProgress {
        schema_digest: projection.schema().digest(),
        owner_store_id: owner,
        source_generation,
        phase,
        cursor_position: replay.last_position(),
        journal_head_digest,
        payload_digest: sha256(&payload),
        invariant_digest: replay.state().invariant_digest(),
        frontier_digest: sha256(&frontier),
        record_count: replay.record_count(),
        payload,
        frontier,
    };
    store.store_progress(projection.schema(), &progress)
}

fn persist_candidate<P: Projection>(
    store: &crate::ProjectionStore,
    projection: &P,
    source_generation: Option<CatalogGeneration>,
    candidate: &RebuildCandidate<P::State>,
) -> Result<(), ProjectionError> {
    let progress = StoredProgress {
        schema_digest: projection.schema().digest(),
        owner_store_id: candidate.owner_store_id(),
        source_generation,
        phase: WorkPhase::Ready,
        cursor_position: candidate.checkpoint().last_position(),
        journal_head_digest: Some(candidate.checkpoint().journal_head_digest()),
        payload_digest: candidate.checkpoint().payload_digest(),
        invariant_digest: candidate.invariant_digest(),
        frontier_digest: candidate.frontier_digest(),
        record_count: candidate.record_count(),
        payload: candidate.payload().to_vec(),
        frontier: candidate.frontier_payload().to_vec(),
    };
    store.store_progress(projection.schema(), &progress)
}

fn resume_after_install_race<P: Projection>(
    store: &crate::ProjectionStore,
    projection: &P,
    owner: StoreId,
    source_generation: &mut Option<CatalogGeneration>,
    candidate: &RebuildCandidate<P::State>,
) -> Result<ReplayCheckpoint<P::State>, ProjectionError> {
    let mut replay = ReplayCheckpoint::restore(
        candidate.payload(),
        candidate.invariant_digest(),
        candidate.frontier_payload(),
        candidate.checkpoint().last_position(),
        candidate.record_count(),
    )?;
    let active = store.load_active(projection.schema())?;
    let observed_source = active.as_ref().map(ActiveGeneration::generation);
    if observed_source != *source_generation {
        if let Some(active) = active.as_ref() {
            if active.checkpoint().last_position() >= replay.last_position() {
                if let Some(restored) = restore_active::<P>(active) {
                    replay = restored;
                }
            }
        }
        *source_generation = observed_source;
    }
    persist_replay(
        store,
        projection,
        owner,
        *source_generation,
        WorkPhase::Folding,
        None,
        &replay,
    )?;
    Ok(replay)
}

fn tip_export(journal: &mut SqliteJournal) -> Result<IntegrityExport, ProjectionError> {
    let observed = journal
        .global_events_after(u64::MAX, 1)
        .map_err(|error| ProjectionError::journal("observe projection journal tip", error))?;
    if observed.latest() == 0 {
        journal
            .integrity_export()
            .map_err(|error| ProjectionError::journal("verify empty projection journal", error))
    } else {
        journal.integrity_export_for_retained_event(observed.latest()).map_err(|error| {
            ProjectionError::journal("verify retained projection journal tip", error)
        })
    }
}

fn verify_owner(owner: StoreId, export: &IntegrityExport) -> Result<(), ProjectionError> {
    if export.report().store_id() == owner {
        Ok(())
    } else {
        Err(ProjectionError::new(
            ProjectionErrorKind::StaleCheckpoint,
            RecoveryClass::CorrectInput,
            "bind projection rebuild owner",
            "verified journal export belongs to another durable store",
        ))
    }
}

fn ensure_live(cancellation: &JournalCancellation) -> Result<(), ProjectionError> {
    if cancellation.is_cancelled() {
        Err(ProjectionError::cancelled())
    } else {
        Ok(())
    }
}
