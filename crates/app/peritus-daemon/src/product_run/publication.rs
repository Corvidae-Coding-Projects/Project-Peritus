//! Joined per-run canonical publication with immutable mutation-lineage tickets.

use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

use peritus_types::{RunId, Sha256Digest};
use sha2::{Digest as _, Sha256};

use super::{
    ProductRunService, ProductRunServiceError, RunRecord, interaction::InteractionOptions,
    progress::RunProgress,
};

const RETRY_DELAY: Duration = Duration::from_millis(50);

/// Whether a caller must observe exact durable coverage before it can report success.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::product_run) enum MutationDisposition {
    Observation,
    DurabilityRequired,
}

impl MutationDisposition {
    const fn tag(self) -> u8 {
        match self {
            Self::Observation => 1,
            Self::DurabilityRequired => 2,
        }
    }
}

/// Stable domain identity for every canonical run mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::product_run) enum RunMutationKind {
    InitialAdmission,
    RetryAdmission,
    ReviewFeedbackAdmission,
    ModelSelection,
    InteractionInput,
    InteractionSource,
    InteractionActivity,
    PreviewAdmission,
    PreviewObservation,
    ExecutionObservation,
    ExecutionFailure,
    Cancellation,
    SettlementObligation,
    SettlementProgress,
    DeliverableReservation,
    DeliverableEffect,
    ImprovementObservation,
    Acknowledge,
    Recovery,
    ShutdownStopping,
    ShutdownSettlement,
}

impl RunMutationKind {
    const fn tag(self) -> u16 {
        match self {
            Self::InitialAdmission => 1,
            Self::RetryAdmission => 2,
            Self::ReviewFeedbackAdmission => 3,
            Self::ModelSelection => 4,
            Self::InteractionInput => 5,
            Self::InteractionSource => 6,
            Self::InteractionActivity => 7,
            Self::PreviewAdmission => 8,
            Self::PreviewObservation => 9,
            Self::ExecutionObservation => 10,
            Self::ExecutionFailure => 11,
            Self::Cancellation => 12,
            Self::SettlementObligation => 13,
            Self::SettlementProgress => 14,
            Self::DeliverableReservation => 15,
            Self::DeliverableEffect => 16,
            Self::ImprovementObservation => 17,
            Self::Acknowledge => 18,
            Self::Recovery => 19,
            Self::ShutdownStopping => 20,
            Self::ShutdownSettlement => 21,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TicketKey {
    run: RunId,
    attempt: u64,
    revision: u64,
    prior_lineage_root: Sha256Digest,
    lineage_root: Sha256Digest,
    disposition: MutationDisposition,
}

struct TicketCompletion {
    result: Mutex<Option<Result<(), ProductRunServiceError>>>,
    ready: Condvar,
}

impl TicketCompletion {
    fn new() -> Self {
        Self { result: Mutex::new(None), ready: Condvar::new() }
    }

    fn complete(&self, result: Result<(), ProductRunServiceError>) {
        if let Ok(mut slot) = self.result.lock()
            && slot.is_none()
        {
            *slot = Some(result);
            self.ready.notify_all();
        }
    }

    fn wait(&self) -> Result<(), ProductRunServiceError> {
        let mut result = self
            .result
            .lock()
            .map_err(|_| ProductRunServiceError::Unavailable)?;
        loop {
            if let Some(result) = result.as_ref() {
                return result.clone();
            }
            result = self
                .ready
                .wait(result)
                .map_err(|_| ProductRunServiceError::Unavailable)?;
        }
    }
}

/// Opaque proof that one exact mutation entered the run's contiguous publication lineage.
pub(in crate::product_run) struct MutationTicket {
    key: TicketKey,
    completion: Arc<TicketCompletion>,
}

impl MutationTicket {
    #[must_use]
    pub(in crate::product_run) const fn run(&self) -> RunId {
        self.key.run
    }

    #[must_use]
    pub(in crate::product_run) const fn revision(&self) -> u64 {
        self.key.revision
    }
}

/// Deep bounded state captured for physical preparation outside the registry.
pub(in crate::product_run) struct RunIdentitySnapshot {
    pub(in crate::product_run) run: RunId,
    pub(in crate::product_run) workspace: peritus_types::WorkspaceId,
    pub(in crate::product_run) conversation:
        peritus_product_runner::control::ConversationId,
    pub(in crate::product_run) actor: [u8; 16],
    pub(in crate::product_run) start:
        peritus_product_runner::control::ControlOperation,
    pub(in crate::product_run) attempt_sequence: u64,
    pub(in crate::product_run) handoff_sequence: u64,
    pub(in crate::product_run) record_revision: u64,
    pub(in crate::product_run) lineage_root: Sha256Digest,
    pub(in crate::product_run) cancelled: Arc<AtomicBool>,
    pub(in crate::product_run) request: super::ProductRunRequest,
    pub(in crate::product_run) interaction: InteractionOptions,
    pub(in crate::product_run) finding_state: String,
    pub(in crate::product_run) finding_head: Sha256Digest,
    pub(in crate::product_run) review_artifacts_externalized: bool,
    pub(in crate::product_run) resume: Option<peritus_product_runner::ProductRunResume>,
    pub(in crate::product_run) snapshot: peritus_app_protocol::ProductRunSnapshot,
    pub(in crate::product_run) progress: RunProgress,
    pub(in crate::product_run) attempt_admission:
        Option<peritus_product_runner::control::OperationId>,
    pub(in crate::product_run) continuation_admissions:
        Vec<peritus_product_runner::control::OperationId>,
    pub(in crate::product_run) continuation_sources: Vec<super::ContinuationSource>,
}

impl RunIdentitySnapshot {
    pub(in crate::product_run) fn matches(&self, record: &RunRecord) -> bool {
        self.run == record.request.run_id()
            && self.workspace == record.request.workspace_id()
            && self.conversation == record.interaction.workbench.conversation()
            && self.actor == *record.interaction.workbench.actor_bytes()
            && self.start == record.interaction.workbench
            && self.attempt_sequence == record.attempt_sequence
            && self.handoff_sequence == record.handoff_sequence
            && self.record_revision == record.record_revision
            && self.lineage_root == record.record_lineage_root
            && Arc::ptr_eq(&self.cancelled, &record.cancelled)
            && self.finding_head == record.finding_catalog.head_digest
            && self.review_artifacts_externalized
                == record.finding_catalog.review_artifacts_externalized
            && self.attempt_admission == record.attempt_admission
            && self.continuation_admissions == record.continuation_admissions
            && self.continuation_sources == record.continuation_sources
    }
}

#[derive(Clone, Copy)]
pub(super) struct PublicationHead {
    pub(super) revision: u64,
    pub(super) lineage_root: Sha256Digest,
    pub(super) canonical_digest: Sha256Digest,
}

struct MutationEntry {
    key: TicketKey,
    kind: RunMutationKind,
    input_digest: Sha256Digest,
    state_digest: Sha256Digest,
    completion: Arc<TicketCompletion>,
}

#[derive(Clone)]
struct CanonicalCandidate {
    run: RunId,
    attempt: u64,
    revision: u64,
    lineage_root: Sha256Digest,
    state_digest: Sha256Digest,
    bytes: Arc<[u8]>,
    digest: Sha256Digest,
}

struct LaneState {
    durable: PublicationHead,
    entries: VecDeque<MutationEntry>,
    candidates: VecDeque<CanonicalCandidate>,
    stopping: bool,
}

#[derive(Default)]
struct MutationGateState {
    active: bool,
    waiters: VecDeque<Arc<()>>,
}

struct PublicationLane {
    run: RunId,
    directory: std::path::PathBuf,
    state: Mutex<LaneState>,
    ready: Condvar,
    mutation: Mutex<MutationGateState>,
    mutation_ready: Condvar,
    worker: Mutex<Option<JoinHandle<()>>>,
}

struct MutationPermit<'a> {
    lane: &'a PublicationLane,
}

impl PublicationLane {
    fn acquire_mutation(
        &self,
        cancellation: &peritus_journal::JournalCancellation,
        allow_cancelled: bool,
    ) -> Result<MutationPermit<'_>, ProductRunServiceError> {
        let token = Arc::new(());
        let mut state = self
            .mutation
            .lock()
            .map_err(|_| ProductRunServiceError::Unavailable)?;
        state.waiters.push_back(Arc::clone(&token));
        loop {
            if cancellation.is_cancelled() && !allow_cancelled {
                if let Some(index) =
                    state.waiters.iter().position(|queued| Arc::ptr_eq(queued, &token))
                {
                    state.waiters.remove(index);
                }
                self.mutation_ready.notify_all();
                return Err(ProductRunServiceError::Unavailable);
            }
            if !state.active
                && state
                    .waiters
                    .front()
                    .is_some_and(|queued| Arc::ptr_eq(queued, &token))
            {
                state.waiters.pop_front();
                state.active = true;
                return Ok(MutationPermit { lane: self });
            }
            let wake = self
                .mutation_ready
                .wait_timeout(state, Duration::from_millis(1))
                .map_err(|_| ProductRunServiceError::Unavailable)?;
            state = wake.0;
        }
    }

    fn enqueue(
        &self,
        entry: MutationEntry,
        candidate: CanonicalCandidate,
    ) -> Result<(), ProductRunServiceError> {
        let mut state = self.state.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
        if state.stopping {
            return Err(ProductRunServiceError::Unavailable);
        }
        let (expected_revision, expected_root) = state
            .entries
            .back()
            .map_or((state.durable.revision, state.durable.lineage_root), |entry| {
                (entry.key.revision, entry.key.lineage_root)
            });
        if entry.key.run != self.run
            || entry.key.revision != expected_revision.checked_add(1).ok_or_else(|| {
                ProductRunServiceError::internal(
                    "enqueue product-run publication",
                    "record revision overflow",
                )
            })?
            || entry.key.prior_lineage_root != expected_root
            || candidate.run != self.run
            || candidate.revision != entry.key.revision
            || candidate.lineage_root != entry.key.lineage_root
            || candidate.state_digest != entry.state_digest
        {
            return Err(ProductRunServiceError::internal(
                "enqueue product-run publication",
                "mutation ticket is not the next contiguous lineage member",
            ));
        }
        state.entries.push_back(entry);
        state.candidates.push_back(candidate);
        self.ready.notify_all();
        Ok(())
    }

    fn start(self: &Arc<Self>) -> Result<(), ProductRunServiceError> {
        let mut worker = self.worker.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
        if worker.is_some() {
            return Ok(());
        }
        let lane = Arc::clone(self);
        let handle = std::thread::Builder::new()
            .name(format!("run-publish-{:?}", self.run))
            .spawn(move || lane.publish_loop())
            .map_err(|error| {
                ProductRunServiceError::persistence("start product-run publisher", error)
            })?;
        *worker = Some(handle);
        Ok(())
    }

    fn publish_loop(&self) {
        loop {
            let work = {
                let mut state = match self.state.lock() {
                    Ok(state) => state,
                    Err(_) => return,
                };
                loop {
                    if let Some(candidate) = state.candidates.back().cloned() {
                        let entries = state
                            .entries
                            .iter()
                            .take_while(|entry| entry.key.revision <= candidate.revision)
                            .map(|entry| LineageEntry {
                                key: entry.key,
                                kind: entry.kind,
                                input_digest: entry.input_digest,
                                state_digest: entry.state_digest,
                            })
                            .collect::<Vec<_>>();
                        break Some((state.durable, candidate, entries, state.stopping));
                    }
                    if state.stopping {
                        break None;
                    }
                    state = match self.ready.wait(state) {
                        Ok(state) => state,
                        Err(_) => return,
                    };
                }
            };
            let Some((base, candidate, entries, stopping)) = work else { return };
            let result = validate_candidate(base, &candidate, &entries).and_then(|()| {
                super::persistence::install_canonical_candidate(
                    &self.directory,
                    candidate.run,
                    candidate.attempt,
                    candidate.revision,
                    candidate.lineage_root,
                    base.revision,
                    base.lineage_root,
                    base.canonical_digest,
                    &candidate.bytes,
                )
            });
            let mut state = match self.state.lock() {
                Ok(state) => state,
                Err(_) => return,
            };
            match result {
                Ok(digest) => {
                    if state.durable.revision != base.revision
                        || state.durable.lineage_root != base.lineage_root
                        || state.durable.canonical_digest != base.canonical_digest
                    {
                        complete_through(
                            &mut state,
                            candidate.revision,
                            Err(ProductRunServiceError::internal(
                                "install product-run publication",
                                "durable publication head changed outside its joined owner",
                            )),
                        );
                        return;
                    }
                    state.durable = PublicationHead {
                        revision: candidate.revision,
                        lineage_root: candidate.lineage_root,
                        canonical_digest: digest,
                    };
                    complete_through(&mut state, candidate.revision, Ok(()));
                    while state
                        .candidates
                        .front()
                        .is_some_and(|queued| queued.revision <= candidate.revision)
                    {
                        state.candidates.pop_front();
                    }
                }
                Err(error) => {
                    for entry in state
                        .entries
                        .iter()
                        .take_while(|entry| entry.key.revision <= candidate.revision)
                    {
                        if entry.key.disposition == MutationDisposition::DurabilityRequired {
                            entry.completion.complete(Err(error.clone()));
                        }
                    }
                    if stopping {
                        for entry in &state.entries {
                            entry.completion.complete(Err(error.clone()));
                        }
                        return;
                    }
                    let wake = match self.ready.wait_timeout(state, RETRY_DELAY) {
                        Ok(wake) => wake,
                        Err(_) => return,
                    };
                    drop(wake);
                }
            }
        }
    }
}

impl Drop for MutationPermit<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.lane.mutation.lock() {
            state.active = false;
            self.lane.mutation_ready.notify_all();
        }
    }
}

#[derive(Clone, Copy)]
struct LineageEntry {
    key: TicketKey,
    kind: RunMutationKind,
    input_digest: Sha256Digest,
    state_digest: Sha256Digest,
}

fn validate_candidate(
    base: PublicationHead,
    candidate: &CanonicalCandidate,
    entries: &[LineageEntry],
) -> Result<(), ProductRunServiceError> {
    let mut revision = base.revision;
    let mut root = base.lineage_root;
    for entry in entries {
        revision = revision.checked_add(1).ok_or_else(|| {
            ProductRunServiceError::internal(
                "validate product-run publication lineage",
                "record revision overflow",
            )
        })?;
        if entry.key.revision != revision || entry.key.prior_lineage_root != root {
            return Err(ProductRunServiceError::internal(
                "validate product-run publication lineage",
                "the retained mutation lineage is gapped or mislinked",
            ));
        }
        let calculated = lineage_root(
            entry.key.run,
            entry.key.attempt,
            entry.key.revision,
            entry.key.prior_lineage_root,
            entry.kind,
            entry.input_digest,
            entry.key.disposition,
            entry.state_digest,
        );
        if calculated != entry.key.lineage_root {
            return Err(ProductRunServiceError::internal(
                "validate product-run publication lineage",
                "a retained mutation entry changed after acceptance",
            ));
        }
        root = calculated;
    }
    if entries.is_empty()
        || revision != candidate.revision
        || root != candidate.lineage_root
        || entries.last().map(|entry| entry.state_digest) != Some(candidate.state_digest)
        || peritus_codec::sha256(&candidate.bytes) != candidate.digest
    {
        return Err(ProductRunServiceError::internal(
            "validate product-run publication candidate",
            "the canonical candidate is not covered by the complete retained lineage",
        ));
    }
    Ok(())
}

fn complete_through(
    state: &mut LaneState,
    revision: u64,
    result: Result<(), ProductRunServiceError>,
) {
    while state
        .entries
        .front()
        .is_some_and(|entry| entry.key.revision <= revision)
    {
        if let Some(entry) = state.entries.pop_front() {
            entry.completion.complete(result.clone());
        }
    }
}

/// Registry of independent per-run publishers and FIFO mutation gates.
pub(super) struct RunPublicationManager {
    directory: std::path::PathBuf,
    lanes: Mutex<BTreeMap<RunId, Arc<PublicationLane>>>,
    stopping: AtomicBool,
}

impl RunPublicationManager {
    pub(super) fn new(directory: std::path::PathBuf) -> Self {
        Self {
            directory,
            lanes: Mutex::new(BTreeMap::new()),
            stopping: AtomicBool::new(false),
        }
    }

    fn lane(
        &self,
        run: RunId,
        initial: PublicationHead,
    ) -> Result<Arc<PublicationLane>, ProductRunServiceError> {
        if self.stopping.load(Ordering::Acquire) {
            return Err(ProductRunServiceError::Unavailable);
        }
        let mut lanes = self.lanes.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
        if self.stopping.load(Ordering::Acquire) {
            return Err(ProductRunServiceError::Unavailable);
        }
        if let Some(lane) = lanes.get(&run) {
            return Ok(Arc::clone(lane));
        }
        let lane = Arc::new(PublicationLane {
            run,
            directory: self.directory.clone(),
            state: Mutex::new(LaneState {
                durable: initial,
                entries: VecDeque::new(),
                candidates: VecDeque::new(),
                stopping: false,
            }),
            ready: Condvar::new(),
            mutation: Mutex::new(MutationGateState::default()),
            mutation_ready: Condvar::new(),
            worker: Mutex::new(None),
        });
        // Start before publishing the lane into the manager. Shutdown takes this same registry
        // lock before enumerating workers, so it can never miss a newly visible publisher.
        lane.start()?;
        lanes.insert(run, Arc::clone(&lane));
        Ok(lane)
    }

    pub(super) fn shutdown_and_join(&self) -> Vec<String> {
        self.stopping.store(true, Ordering::Release);
        let lanes = match self.lanes.lock() {
            Ok(lanes) => lanes.values().cloned().collect::<Vec<_>>(),
            Err(_) => return vec!["the run publication registry was poisoned".to_owned()],
        };
        for lane in &lanes {
            if let Ok(mut state) = lane.state.lock() {
                state.stopping = true;
                lane.ready.notify_all();
                lane.mutation_ready.notify_all();
            }
        }
        let mut failures = Vec::new();
        for lane in lanes {
            let handle = lane.worker.lock().ok().and_then(|mut worker| worker.take());
            if let Some(handle) = handle
                && handle.join().is_err()
            {
                failures.push(format!("publisher for run {:?} panicked", lane.run));
            }
        }
        failures
    }
}

impl Drop for RunPublicationManager {
    fn drop(&mut self) {
        let _ = self.shutdown_and_join();
    }
}

impl ProductRunService {
    pub(in crate::product_run) fn capture_run_identity(
        &self,
        run: RunId,
    ) -> Result<RunIdentitySnapshot, ProductRunServiceError> {
        let records = self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        let record = records.get(&run).ok_or(ProductRunServiceError::NotFound)?;
        Ok(RunIdentitySnapshot {
            run,
            workspace: record.request.workspace_id(),
            conversation: record.interaction.workbench.conversation(),
            actor: *record.interaction.workbench.actor_bytes(),
            start: record.interaction.workbench.clone(),
            attempt_sequence: record.attempt_sequence,
            handoff_sequence: record.handoff_sequence,
            record_revision: record.record_revision,
            lineage_root: record.record_lineage_root,
            cancelled: Arc::clone(&record.cancelled),
            request: record.request.clone(),
            interaction: record.interaction.clone(),
            finding_state: record.finding_state.clone(),
            finding_head: record.finding_catalog.head_digest,
            review_artifacts_externalized:
                record.finding_catalog.review_artifacts_externalized,
            resume: record.resume.clone(),
            snapshot: record.snapshot.clone(),
            progress: record.progress.clone(),
            attempt_admission: record.attempt_admission,
            continuation_admissions: record.continuation_admissions.clone(),
            continuation_sources: record.continuation_sources.clone(),
        })
    }

    pub(in crate::product_run) fn mutate_run<T>(
        &self,
        run: RunId,
        expected_attempt: Option<&Arc<AtomicBool>>,
        kind: RunMutationKind,
        input_digest: Sha256Digest,
        disposition: MutationDisposition,
        mutation: impl FnOnce(&mut RunRecord) -> Result<T, ProductRunServiceError>,
    ) -> Result<(T, MutationTicket), ProductRunServiceError> {
        let mut initial = {
            let records =
                self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
            let record = records.get(&run).ok_or(ProductRunServiceError::NotFound)?;
            PublicationHead {
                revision: record.durable_record_revision,
                lineage_root: record.durable_lineage_root,
                canonical_digest: record.durable_canonical_digest,
            }
        };
        initial.canonical_digest =
            super::persistence::canonical_record_digest(&self.inner.directory, run)?;
        let lane = self.inner.publications.lane(run, initial)?;
        // Closing foreground admission must not revoke an already-admitted mutation whose
        // caller is required to wait for durable coverage. Shutdown joins those owners before
        // stopping this publisher. Best-effort observations remain cancellable immediately.
        let allow_cancelled = disposition == MutationDisposition::DurabilityRequired;
        let _permit = lane.acquire_mutation(&self.inner.control_shutdown, allow_cancelled)?;
        let (base_revision, base_root, base_attempt, base_cancelled, mut draft) = {
            let records =
                self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
            let record = records.get(&run).ok_or(ProductRunServiceError::NotFound)?;
            if expected_attempt.is_some_and(|attempt| !Arc::ptr_eq(attempt, &record.cancelled)) {
                return Err(ProductRunServiceError::InvalidState);
            }
            (
                record.record_revision,
                record.record_lineage_root,
                record.attempt_sequence,
                Arc::clone(&record.cancelled),
                record.clone(),
            )
        };
        let result = mutation(&mut draft)?;
        let revision = base_revision.checked_add(1).ok_or_else(|| {
            ProductRunServiceError::internal(
                "prepare product-run mutation",
                "record revision overflow",
            )
        })?;
        draft.record_revision = revision;
        let canonical = super::persistence::CanonicalRecordCapture::capture(&draft)?;
        let state_digest = canonical.state_digest()?;
        let root = lineage_root(
            run,
            draft.attempt_sequence,
            revision,
            base_root,
            kind,
            input_digest,
            disposition,
            state_digest,
        );
        draft.record_lineage_root = root;
        let bytes = canonical.encode(revision, root)?;
        let digest = peritus_codec::sha256(&bytes);
        let completion = Arc::new(TicketCompletion::new());
        let key = TicketKey {
            run,
            attempt: draft.attempt_sequence,
            revision,
            prior_lineage_root: base_root,
            lineage_root: root,
            disposition,
        };
        let entry = MutationEntry {
            key,
            kind,
            input_digest,
            state_digest,
            completion: Arc::clone(&completion),
        };
        let candidate = CanonicalCandidate {
            run,
            attempt: draft.attempt_sequence,
            revision,
            lineage_root: root,
            state_digest,
            bytes: Arc::from(bytes),
            digest,
        };
        {
            let mut records =
                self.inner.records.write().map_err(|_| ProductRunServiceError::Unavailable)?;
            let record = records.get_mut(&run).ok_or(ProductRunServiceError::NotFound)?;
            if record.record_revision != base_revision
                || record.record_lineage_root != base_root
                || record.attempt_sequence != base_attempt
                || !Arc::ptr_eq(&record.cancelled, &base_cancelled)
                || expected_attempt.is_some_and(|attempt| !Arc::ptr_eq(attempt, &record.cancelled))
            {
                return Err(ProductRunServiceError::InvalidState);
            }
            lane.enqueue(entry, candidate)?;
            *record = draft;
        }
        Ok((result, MutationTicket { key, completion }))
    }

    pub(in crate::product_run) fn await_run_durable(
        &self,
        ticket: MutationTicket,
    ) -> Result<(), ProductRunServiceError> {
        if ticket.key.disposition != MutationDisposition::DurabilityRequired {
            return Err(ProductRunServiceError::internal(
                "await product-run durability",
                "observation tickets do not authorize a durability acknowledgement",
            ));
        }
        ticket.completion.wait()
    }
}

fn lineage_root(
    run: RunId,
    attempt: u64,
    revision: u64,
    prior: Sha256Digest,
    kind: RunMutationKind,
    input_digest: Sha256Digest,
    disposition: MutationDisposition,
    state_digest: Sha256Digest,
) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus-product-run-mutation-lineage-v1\0");
    hasher.update(run.as_bytes());
    hasher.update(attempt.to_be_bytes());
    hasher.update(revision.to_be_bytes());
    hasher.update(prior.into_bytes());
    hasher.update(kind.tag().to_be_bytes());
    hasher.update(input_digest.into_bytes());
    hasher.update([disposition.tag()]);
    hasher.update(state_digest.into_bytes());
    Sha256Digest::new(hasher.finalize().into())
}

/// Advances a canonical record during single-owner startup, before publication lanes exist.
///
/// Startup migration and workspace reconciliation are authoritative mutations too. Publishing
/// them through this path keeps the in-memory durable head aligned with the exact canonical bytes
/// that the foreground manager will later use as its compare-and-swap predecessor.
pub(super) fn persist_startup_record(
    directory: &std::path::Path,
    record: &mut RunRecord,
    input_digest: Sha256Digest,
) -> Result<(), ProductRunServiceError> {
    if record.record_revision != record.durable_record_revision
        || record.record_lineage_root != record.durable_lineage_root
    {
        return Err(ProductRunServiceError::InvalidState);
    }
    let zero = Sha256Digest::new([0; 32]);
    let canonical_digest = super::persistence::canonical_record_digest(
        directory,
        record.request.run_id(),
    )?;
    if canonical_digest != zero && canonical_digest != record.durable_canonical_digest {
        return Err(ProductRunServiceError::internal(
            "publish startup product-run mutation",
            "the canonical record changed after its startup snapshot was loaded",
        ));
    }
    let base_revision = record.record_revision;
    let base_root = record.record_lineage_root;
    let revision = base_revision.checked_add(1).ok_or_else(|| {
        ProductRunServiceError::internal(
            "publish startup product-run mutation",
            "record revision overflow",
        )
    })?;
    record.record_revision = revision;
    let canonical = match super::persistence::CanonicalRecordCapture::capture(record) {
        Ok(canonical) => canonical,
        Err(error) => {
            record.record_revision = base_revision;
            return Err(error);
        }
    };
    let state_digest = match canonical.state_digest() {
        Ok(digest) => digest,
        Err(error) => {
            record.record_revision = base_revision;
            return Err(error);
        }
    };
    let root = lineage_root(
        record.request.run_id(),
        record.attempt_sequence,
        revision,
        base_root,
        RunMutationKind::Recovery,
        input_digest,
        MutationDisposition::DurabilityRequired,
        state_digest,
    );
    record.record_lineage_root = root;
    let bytes = match canonical.encode(revision, root) {
        Ok(bytes) => bytes,
        Err(error) => {
            record.record_revision = base_revision;
            record.record_lineage_root = base_root;
            return Err(error);
        }
    };
    let expected = if canonical_digest == zero {
        PublicationHead { revision: 0, lineage_root: zero, canonical_digest: zero }
    } else {
        PublicationHead {
            revision: record.durable_record_revision,
            lineage_root: record.durable_lineage_root,
            canonical_digest,
        }
    };
    let installed = super::persistence::install_canonical_candidate(
        directory,
        record.request.run_id(),
        record.attempt_sequence,
        revision,
        root,
        expected.revision,
        expected.lineage_root,
        expected.canonical_digest,
        &bytes,
    );
    match installed {
        Ok(digest) => {
            record.durable_record_revision = revision;
            record.durable_lineage_root = root;
            record.durable_canonical_digest = digest;
            Ok(())
        }
        Err(error) => {
            record.record_revision = base_revision;
            record.record_lineage_root = base_root;
            Err(error)
        }
    }
}
