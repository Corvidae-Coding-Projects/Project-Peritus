//! Dynamically growing live replay ownership backed by an indexed historical receipt store.

use std::collections::{BTreeMap, HashMap, hash_map::Entry};
use std::fmt;

use peritus_tool_protocol::{
    IdempotencySemantics, PreparedToolCall, ProgressContract, ToolProgress, ToolResult,
};
use peritus_types::{ActionId, Sha256Digest};

use crate::{DispatchOutcome, ReplayDisposition, RouterError, RouterErrorKind};

/// One immutable, digest-chained V2 progress page.
#[derive(Clone, Debug)]
pub struct ProgressPage {
    action_id: ActionId,
    replay_identity: Sha256Digest,
    prepared_digest: Sha256Digest,
    start: u64,
    end: u64,
    previous_digest: Option<Sha256Digest>,
    digest: Sha256Digest,
    canonical_events: Vec<Vec<u8>>,
    events: Vec<ToolProgress>,
}

impl ProgressPage {
    fn new(
        prepared: &PreparedToolCall,
        start: u64,
        previous_digest: Option<Sha256Digest>,
        events: Vec<ToolProgress>,
    ) -> Result<Self, ReplayStoreError> {
        if prepared.call().limits().progress_contract() != ProgressContract::PagedV2
            || events.is_empty()
            || events.first().is_none_or(|event| event.sequence() != start)
            || events.windows(2).any(|pair| {
                pair[0].sequence().checked_add(1) != Some(pair[1].sequence())
            })
            || events.iter().any(|event| {
                event.action_id() != prepared.call().action_id()
                    || event.prepared_digest() != prepared.prepared_digest()
                    || event.progress_contract() != ProgressContract::PagedV2
            })
        {
            return Err(ReplayStoreError::integrity(
                "progress page is not V2, is empty, discontinuous, or bound to another call",
            ));
        }
        let observed = u64::try_from(events.len())
            .map_err(|_| ReplayStoreError::integrity("progress page length is not representable"))?;
        let end = start
            .checked_add(observed)
            .ok_or_else(|| ReplayStoreError::integrity("progress frontier overflowed"))?;
        let canonical_events = events.iter().map(ToolProgress::canonical_bytes).collect();
        let mut page = Self {
            action_id: prepared.call().action_id(),
            replay_identity: prepared.replay_identity().digest(),
            prepared_digest: prepared.prepared_digest(),
            start,
            end,
            previous_digest,
            digest: Sha256Digest::new([0; 32]),
            canonical_events,
            events,
        };
        page.digest = page.compute_digest();
        Ok(page)
    }

    fn compute_digest(&self) -> Sha256Digest {
        let mut bytes = Vec::with_capacity(160);
        bytes.extend_from_slice(b"peritus.tool-progress-page.v1\0");
        bytes.extend_from_slice(self.action_id.as_bytes());
        bytes.extend_from_slice(self.replay_identity.as_bytes());
        bytes.extend_from_slice(self.prepared_digest.as_bytes());
        bytes.extend_from_slice(&self.start.to_be_bytes());
        bytes.extend_from_slice(&self.end.to_be_bytes());
        match self.previous_digest {
            Some(digest) => {
                bytes.push(1);
                bytes.extend_from_slice(digest.as_bytes());
            }
            None => bytes.push(0),
        }
        bytes.extend_from_slice(&(self.canonical_events.len() as u64).to_be_bytes());
        for encoded in &self.canonical_events {
            bytes.extend_from_slice(&(encoded.len() as u64).to_be_bytes());
            bytes.extend_from_slice(encoded);
        }
        peritus_codec::sha256(&bytes)
    }

    /// Returns the producing action.
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }
    /// Returns the exact replay identity.
    #[must_use]
    pub const fn replay_identity(&self) -> Sha256Digest {
        self.replay_identity
    }
    /// Returns the prepared-call digest.
    #[must_use]
    pub const fn prepared_digest(&self) -> Sha256Digest {
        self.prepared_digest
    }
    /// Returns the inclusive page start frontier.
    #[must_use]
    pub const fn start(&self) -> u64 {
        self.start
    }
    /// Returns the exclusive next frontier.
    #[must_use]
    pub const fn end(&self) -> u64 {
        self.end
    }
    /// Returns the preceding page digest, if this is not the first page.
    #[must_use]
    pub const fn previous_digest(&self) -> Option<Sha256Digest> {
        self.previous_digest
    }
    /// Returns this page's exact digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Borrows the exact ordered events.
    #[must_use]
    pub fn events(&self) -> &[ToolProgress] {
        &self.events
    }

    /// Borrows exact canonical event envelopes for durable transport and restart readers.
    #[must_use]
    pub fn canonical_events(&self) -> &[Vec<u8>] {
        &self.canonical_events
    }

    /// Reconstructs one page from trusted durable fields and exact canonical event envelopes.
    ///
    /// The reader decodes every typed event while retaining the exact canonical bytes used for
    /// page identity and duplicate reconciliation.
    pub fn recovered(
        action_id: ActionId,
        replay_identity: Sha256Digest,
        prepared_digest: Sha256Digest,
        start: u64,
        end: u64,
        previous_digest: Option<Sha256Digest>,
        digest: Sha256Digest,
        canonical_events: Vec<Vec<u8>>,
    ) -> Result<Self, ReplayStoreError> {
        let count = u64::try_from(canonical_events.len())
            .map_err(|_| ReplayStoreError::integrity("progress page length is not representable"))?;
        let events = canonical_events
            .iter()
            .map(|bytes| {
                ToolProgress::from_canonical_bytes(bytes).map_err(|_| {
                    ReplayStoreError::integrity("durable progress event envelope is invalid")
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let page = Self {
            action_id,
            replay_identity,
            prepared_digest,
            start,
            end,
            previous_digest,
            digest,
            canonical_events,
            events,
        };
        if start.checked_add(count) != Some(end)
            || page.compute_digest() != digest
            || page.events.first().is_none_or(|event| event.sequence() != start)
            || page.events.windows(2).any(|pair| {
                pair[0].sequence().checked_add(1) != Some(pair[1].sequence())
            })
            || page.events.iter().any(|event| {
                event.action_id() != action_id
                    || event.prepared_digest() != prepared_digest
                    || event.progress_contract() != ProgressContract::PagedV2
            })
        {
            return Err(ReplayStoreError::integrity(
                "durable progress page has an invalid frontier or digest",
            ));
        }
        Ok(page)
    }
}

impl PartialEq for ProgressPage {
    fn eq(&self, other: &Self) -> bool {
        self.action_id == other.action_id
            && self.replay_identity == other.replay_identity
            && self.prepared_digest == other.prepared_digest
            && self.start == other.start
            && self.end == other.end
            && self.previous_digest == other.previous_digest
            && self.digest == other.digest
            && self.canonical_events == other.canonical_events
    }
}

impl Eq for ProgressPage {}

/// Durable phase of one exact replay identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayRecordKind {
    /// Authority was accepted and the one-use identity was consumed before dispatch.
    Reserved,
    /// The dispatcher returned an owned active execution.
    Active,
    /// A terminal result exists but the descriptor permits only reporting the prior outcome.
    NonIdempotentTerminal,
    /// A terminal result may be returned for an exact replay.
    ReplayTerminal,
    /// The effect outcome cannot safely be inferred or repeated.
    Indeterminate,
}

/// Immutable durable replay receipt for one action identity and exact bound call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayRecord {
    action_id: ActionId,
    replay_identity: Sha256Digest,
    reservation_owner: Option<ReplayReservationOwner>,
    kind: ReplayRecordKind,
    terminal_bytes: Option<Vec<u8>>,
    terminal: Option<Box<ToolResult>>,
}

impl ReplayRecord {
    /// Reconstructs a validated non-replayable receipt from durable indexed bytes.
    ///
    /// Exact replay-terminal reconstruction requires the typed terminal envelope and is therefore
    /// intentionally left to stores that retain that type directly.
    ///
    /// # Errors
    ///
    /// Rejects terminal bytes on pending/indeterminate phases, missing terminal bytes on a
    /// terminal phase, or an attempt to reconstruct a replayable terminal without its type.
    pub fn recovered(
        action_id: ActionId,
        replay_identity: Sha256Digest,
        reservation_owner: Option<ReplayReservationOwner>,
        kind: ReplayRecordKind,
        terminal_bytes: Option<Vec<u8>>,
    ) -> Result<Self, ReplayStoreError> {
        let valid = match kind {
            ReplayRecordKind::Reserved
            | ReplayRecordKind::Active
            | ReplayRecordKind::Indeterminate => terminal_bytes.is_none(),
            ReplayRecordKind::NonIdempotentTerminal => {
                terminal_bytes.as_ref().is_some_and(|bytes| !bytes.is_empty())
            }
            ReplayRecordKind::ReplayTerminal => false,
        };
        if !valid {
            return Err(ReplayStoreError::integrity(
                "durable replay receipt has inconsistent phase and terminal bytes",
            ));
        }
        Ok(Self {
            action_id,
            replay_identity,
            reservation_owner,
            kind,
            terminal_bytes,
            terminal: None,
        })
    }

    const fn reservation(
        action_id: ActionId,
        replay_identity: Sha256Digest,
        reservation_owner: ReplayReservationOwner,
    ) -> Self {
        Self {
            action_id,
            replay_identity,
            reservation_owner: Some(reservation_owner),
            kind: ReplayRecordKind::Reserved,
            terminal_bytes: None,
            terminal: None,
        }
    }

    const fn active(
        prepared: &PreparedToolCall,
        reservation_owner: ReplayReservationOwner,
    ) -> Self {
        Self::pending(prepared, reservation_owner, ReplayRecordKind::Active)
    }

    const fn pending(
        prepared: &PreparedToolCall,
        reservation_owner: ReplayReservationOwner,
        kind: ReplayRecordKind,
    ) -> Self {
        Self {
            action_id: prepared.call().action_id(),
            replay_identity: prepared.replay_identity().digest(),
            reservation_owner: Some(reservation_owner),
            kind,
            terminal_bytes: None,
            terminal: None,
        }
    }

    fn terminal(
        prepared: &PreparedToolCall,
        reservation_owner: ReplayReservationOwner,
        result: ToolResult,
    ) -> Self {
        let terminal_bytes = Some(result.canonical_bytes());
        let (kind, terminal) = match prepared.descriptor().idempotency() {
            IdempotencySemantics::ReplayTerminal => {
                (ReplayRecordKind::ReplayTerminal, Some(Box::new(result)))
            }
            IdempotencySemantics::ReportPriorOutcome => {
                (ReplayRecordKind::NonIdempotentTerminal, None)
            }
        };
        Self {
            action_id: prepared.call().action_id(),
            replay_identity: prepared.replay_identity().digest(),
            reservation_owner: Some(reservation_owner),
            kind,
            terminal_bytes,
            terminal,
        }
    }

    const fn indeterminate(
        prepared: &PreparedToolCall,
        reservation_owner: ReplayReservationOwner,
    ) -> Self {
        Self::pending(prepared, reservation_owner, ReplayRecordKind::Indeterminate)
    }

    /// Returns the exact action identity.
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }

    /// Returns the exact replay-identity digest.
    #[must_use]
    pub const fn replay_identity(&self) -> Sha256Digest {
        self.replay_identity
    }

    /// Returns the durable reservation owner, or `None` for a legacy ownerless receipt.
    #[must_use]
    pub const fn reservation_owner(&self) -> Option<ReplayReservationOwner> {
        self.reservation_owner
    }

    /// Returns the durable lifecycle phase.
    #[must_use]
    pub const fn kind(&self) -> ReplayRecordKind {
        self.kind
    }

    /// Borrows canonical terminal bytes when the phase is terminal.
    #[must_use]
    pub fn terminal_bytes(&self) -> Option<&[u8]> {
        self.terminal_bytes.as_deref()
    }

    fn outcome(&self, prepared: &PreparedToolCall) -> Result<DispatchOutcome, RouterError> {
        if self.action_id != prepared.call().action_id()
            || self.replay_identity != prepared.replay_identity().digest()
        {
            return Err(RouterError::new(
                RouterErrorKind::ReplayConflict,
                "inspect tool replay",
                "action identity was reused with different bound bytes",
            ));
        }
        Ok(match self.kind {
            ReplayRecordKind::Reserved if self.reservation_owner.is_none() => {
                DispatchOutcome::PriorOutcome(ReplayDisposition::Indeterminate)
            }
            ReplayRecordKind::Reserved | ReplayRecordKind::Active => {
                DispatchOutcome::PriorOutcome(ReplayDisposition::Active)
            }
            ReplayRecordKind::NonIdempotentTerminal => {
                DispatchOutcome::PriorOutcome(ReplayDisposition::NonIdempotentTerminal)
            }
            ReplayRecordKind::ReplayTerminal => {
                let Some(result) = &self.terminal else {
                    return Err(durability_error());
                };
                DispatchOutcome::Replayed(result.as_ref().clone())
            }
            ReplayRecordKind::Indeterminate => {
                DispatchOutcome::PriorOutcome(ReplayDisposition::Indeterminate)
            }
        })
    }
}

/// Unguessable durable owner of one reservation attempt.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ReplayReservationOwner([u8; Self::LENGTH]);

impl ReplayReservationOwner {
    /// Reservation-owner representation width.
    pub const LENGTH: usize = 32;

    /// Reconstructs exact owner bytes read from a trusted durable store.
    #[must_use]
    pub const fn new(bytes: [u8; Self::LENGTH]) -> Self {
        Self(bytes)
    }

    /// Borrows the exact owner bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; Self::LENGTH] {
        &self.0
    }
}

impl fmt::Debug for ReplayReservationOwner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ReplayReservationOwner([REDACTED])")
    }
}

/// One stable reservation operation that may be retried after an ambiguous durable append.
#[must_use = "a reservation attempt must be submitted or deliberately abandoned"]
#[derive(Debug)]
pub struct ReplayReservationAttempt {
    record: Box<ReplayRecord>,
    resolved: bool,
}

impl ReplayReservationAttempt {
    /// Creates a fresh unguessable owner for one exact reservation operation.
    ///
    /// # Errors
    ///
    /// Returns a storage failure when operating-system randomness is unavailable.
    pub fn new(
        action_id: ActionId,
        replay_identity: Sha256Digest,
    ) -> Result<Self, ReplayStoreError> {
        let mut owner = [0_u8; ReplayReservationOwner::LENGTH];
        getrandom::fill(&mut owner).map_err(|error| {
            ReplayStoreError::new(format!("create replay reservation owner: {error}"))
        })?;
        Ok(Self {
            record: Box::new(ReplayRecord::reservation(
                action_id,
                replay_identity,
                ReplayReservationOwner::new(owner),
            )),
            resolved: false,
        })
    }
}

/// Move-only proof that one exact replay identity was durably reserved before dispatch.
#[must_use = "a durable replay reservation must be dispatched or deliberately abandoned"]
#[derive(Debug)]
pub struct ReplayReservation {
    record: Box<ReplayRecord>,
}

/// Move-only copy of a settled receipt still awaiting durable publication.
#[must_use = "a pending replay receipt must be published before it is acknowledged"]
#[derive(Debug)]
pub struct PendingReplayReceipt {
    record: Box<ReplayRecord>,
}

/// Move-only proof that a settled receipt was accepted by its durable store.
#[must_use = "a published replay receipt must be acknowledged by its owning router"]
#[derive(Debug)]
pub struct PublishedReplayReceipt {
    record: Box<ReplayRecord>,
}

/// Result of one immutable replay-store append.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayAppendOutcome {
    /// This call appended a new immutable phase.
    Appended,
    /// The exact phase was already durable before this call.
    AlreadyPresent,
}

/// Stable replay-store failure category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayStoreErrorKind {
    /// Another owner currently holds the durable store lock.
    Contended,
    /// A commit may have succeeded and must be reconciled with the same operation owner.
    AmbiguousCommit,
    /// The exact action identity was already reserved by another completed append.
    Occupied,
    /// Storage could not be read or written.
    Storage,
    /// Stored replay history is malformed or non-monotonic.
    Integrity,
}

/// Dynamic storage failure retained at the replay-index boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayStoreError {
    kind: ReplayStoreErrorKind,
    detail: String,
}

impl ReplayStoreError {
    /// Creates a bounded replay-store diagnostic.
    #[must_use]
    pub fn new(detail: impl Into<String>) -> Self {
        Self::with_kind(ReplayStoreErrorKind::Storage, detail)
    }

    /// Creates a recoverable store-contention diagnostic.
    #[must_use]
    pub fn contended(detail: impl Into<String>) -> Self {
        Self::with_kind(ReplayStoreErrorKind::Contended, detail)
    }

    /// Creates an ambiguous-commit diagnostic that is safe to retry only with the same owner.
    #[must_use]
    pub fn ambiguous_commit(detail: impl Into<String>) -> Self {
        Self::with_kind(ReplayStoreErrorKind::AmbiguousCommit, detail)
    }

    /// Creates a fail-closed already-reserved diagnostic.
    #[must_use]
    pub fn occupied(detail: impl Into<String>) -> Self {
        Self::with_kind(ReplayStoreErrorKind::Occupied, detail)
    }

    /// Creates a fail-closed replay-integrity diagnostic.
    #[must_use]
    pub fn integrity(detail: impl Into<String>) -> Self {
        Self::with_kind(ReplayStoreErrorKind::Integrity, detail)
    }

    fn with_kind(kind: ReplayStoreErrorKind, detail: impl Into<String>) -> Self {
        let mut detail = detail.into();
        if detail.len() > 1_024 {
            let mut end = 1_024;
            while !detail.is_char_boundary(end) {
                end -= 1;
            }
            detail.truncate(end);
        }
        Self { kind, detail }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> ReplayStoreErrorKind {
        self.kind
    }

    /// Borrows the storage diagnostic.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for ReplayStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for ReplayStoreError {}

/// Indexed monotonic storage for exact replay receipts.
pub trait ReplayStore: Send {
    /// Returns the latest immutable receipt for an action.
    ///
    /// # Errors
    ///
    /// Returns a storage or integrity error rather than treating unreadable history as absent.
    fn lookup(
        &mut self,
        action_id: ActionId,
    ) -> Result<Option<ReplayRecord>, ReplayStoreError>;

    /// Appends a monotonic receipt transition, accepting an exact duplicate idempotently.
    ///
    /// # Errors
    ///
    /// Rejects storage failure, identity conflict, or a non-monotonic transition.
    fn append(
        &mut self,
        record: &ReplayRecord,
    ) -> Result<ReplayAppendOutcome, ReplayStoreError>;

    /// Returns the latest immutable V2 progress page for an action.
    ///
    /// Stores that predate paged progress reject V2 publication explicitly instead of accepting
    /// an observation they cannot recover.
    fn latest_progress(
        &mut self,
        _action_id: ActionId,
    ) -> Result<Option<ProgressPage>, ReplayStoreError> {
        Ok(None)
    }

    /// Returns the immutable page beginning at an exact frontier.
    fn progress_page(
        &mut self,
        _action_id: ActionId,
        _start: u64,
    ) -> Result<Option<ProgressPage>, ReplayStoreError> {
        Ok(None)
    }

    /// Appends one digest-chained progress page, accepting an exact duplicate idempotently.
    fn append_progress(
        &mut self,
        _page: &ProgressPage,
    ) -> Result<ReplayAppendOutcome, ReplayStoreError> {
        Err(ReplayStoreError::integrity(
            "replay store does not support version-two progress pages",
        ))
    }
}

/// Durably consumes one exact replay identity before an external effect can be dispatched.
///
/// A reservation owned by another attempt rejects rather than minting a second dispatch proof.
/// Repeating the same unguessable attempt reconciles an ambiguous append without widening the
/// one-use permit.
///
/// # Errors
///
/// Returns a typed store failure for contention, an existing owner, storage, or integrity faults.
pub fn reserve_replay(
    store: &mut dyn ReplayStore,
    attempt: &mut ReplayReservationAttempt,
) -> Result<ReplayReservation, ReplayStoreError> {
    if attempt.resolved {
        return Err(ReplayStoreError::occupied(
            "replay reservation attempt already yielded its dispatch proof",
        ));
    }
    let record = attempt.record.as_ref();
    match store.append(record) {
        Ok(ReplayAppendOutcome::Appended | ReplayAppendOutcome::AlreadyPresent) => {}
        Err(error) => match store.lookup(record.action_id) {
            Ok(Some(stored)) if stored == *record => {}
            Ok(Some(_)) => {
                return Err(ReplayStoreError::occupied(
                    "tool action identity is already owned by another reservation",
                ));
            }
            Ok(None) | Err(_) => return Err(error),
        },
    }
    attempt.resolved = true;
    Ok(ReplayReservation { record: attempt.record.clone() })
}

/// Publishes one router-owned settled receipt without holding the router's live-state lock.
///
/// # Errors
///
/// Returns a typed store failure. The router retains its original pending receipt, so callers may
/// retry contention with the same logical operation and acknowledge only the returned proof.
pub fn publish_replay_receipt(
    store: &mut dyn ReplayStore,
    pending: &PendingReplayReceipt,
) -> Result<PublishedReplayReceipt, ReplayStoreError> {
    let _ = store.append(&pending.record)?;
    Ok(PublishedReplayReceipt { record: pending.record.clone() })
}

struct MemoryReplayStore {
    records: BTreeMap<ActionId, ReplayRecord>,
    progress: BTreeMap<(ActionId, u64), ProgressPage>,
}

impl MemoryReplayStore {
    const fn new() -> Self {
        Self { records: BTreeMap::new(), progress: BTreeMap::new() }
    }
}

impl ReplayStore for MemoryReplayStore {
    fn lookup(
        &mut self,
        action_id: ActionId,
    ) -> Result<Option<ReplayRecord>, ReplayStoreError> {
        Ok(self.records.get(&action_id).cloned())
    }

    fn append(
        &mut self,
        record: &ReplayRecord,
    ) -> Result<ReplayAppendOutcome, ReplayStoreError> {
        if let Some(previous) = self.records.get(&record.action_id) {
            if previous == record {
                return Ok(ReplayAppendOutcome::AlreadyPresent);
            }
            if record.kind == ReplayRecordKind::Reserved {
                return Err(ReplayStoreError::occupied(
                    "tool action identity is already reserved by another owner",
                ));
            }
            validate_transition(previous, record)?;
        } else if record.kind != ReplayRecordKind::Reserved {
            return Err(ReplayStoreError::integrity(
                "replay receipt history does not begin with a reservation",
            ));
        }
        self.records.insert(record.action_id, record.clone());
        Ok(ReplayAppendOutcome::Appended)
    }

    fn latest_progress(
        &mut self,
        action_id: ActionId,
    ) -> Result<Option<ProgressPage>, ReplayStoreError> {
        Ok(self
            .progress
            .range((action_id, 0)..=(action_id, u64::MAX))
            .next_back()
            .map(|(_, page)| page.clone()))
    }

    fn progress_page(
        &mut self,
        action_id: ActionId,
        start: u64,
    ) -> Result<Option<ProgressPage>, ReplayStoreError> {
        Ok(self.progress.get(&(action_id, start)).cloned())
    }

    fn append_progress(
        &mut self,
        page: &ProgressPage,
    ) -> Result<ReplayAppendOutcome, ReplayStoreError> {
        if let Some(stored) = self.progress.get(&(page.action_id, page.start)) {
            return if stored == page {
                Ok(ReplayAppendOutcome::AlreadyPresent)
            } else {
                Err(ReplayStoreError::integrity(
                    "progress page start is already bound to different bytes",
                ))
            };
        }
        let previous = self
            .progress
            .range((page.action_id, 0)..=(page.action_id, u64::MAX))
            .next_back()
            .map(|(_, value)| value);
        let valid = match previous {
            Some(previous) => {
                previous.replay_identity == page.replay_identity
                    && previous.prepared_digest == page.prepared_digest
                    && previous.end == page.start
                    && page.previous_digest == Some(previous.digest)
            }
            None => page.start == 0 && page.previous_digest.is_none(),
        };
        if !valid || page.digest != page.compute_digest() {
            return Err(ReplayStoreError::integrity(
                "progress page breaks its identity, frontier, or digest chain",
            ));
        }
        self.progress.insert((page.action_id, page.start), page.clone());
        Ok(ReplayAppendOutcome::Appended)
    }
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "the sibling router module owns this crate-internal ledger type"
)]
pub(crate) struct ReplayLedger {
    entries: HashMap<ActionId, ReplayRecord>,
    store: Box<dyn ReplayStore>,
}

impl ReplayLedger {
    pub(crate) fn new(initial_page: usize) -> Self {
        Self::with_store(initial_page, Box::new(MemoryReplayStore::new()))
    }

    pub(crate) fn with_store(initial_page: usize, store: Box<dyn ReplayStore>) -> Self {
        Self { entries: HashMap::with_capacity(initial_page), store }
    }

    pub(crate) fn inspect(
        &mut self,
        prepared: &PreparedToolCall,
    ) -> Result<Option<(DispatchOutcome, bool)>, RouterError> {
        if let Some(record) = self.entries.get(&prepared.call().action_id()).cloned() {
            self.publish_settled(&record);
            return record
                .outcome(prepared)
                .map(|outcome| Some((outcome, is_settled(record.kind))));
        }
        self.store
            .lookup(prepared.call().action_id())
            .map_err(|_| durability_error())?
            .as_ref()
            .map(|record| {
                record
                    .outcome(prepared)
                    .map(|outcome| (outcome, is_settled(record.kind)))
            })
            .transpose()
    }

    pub(crate) fn reconnect(
        &mut self,
        prepared: &PreparedToolCall,
    ) -> Result<Option<ReplayRecord>, RouterError> {
        let record = match self.entries.get(&prepared.call().action_id()).cloned() {
            Some(record) => Some(record),
            None => self
                .store
                .lookup(prepared.call().action_id())
                .map_err(|_| durability_error())?,
        };
        if record.as_ref().is_some_and(|record| {
            record.action_id != prepared.call().action_id()
                || record.replay_identity != prepared.replay_identity().digest()
        }) {
            return Err(RouterError::new(
                RouterErrorKind::ReplayConflict,
                "reconnect durable tool invocation",
                "durable replay receipt differs from the prepared call",
            ));
        }
        Ok(record)
    }

    pub(crate) fn active_adoption(
        &mut self,
        prepared: &PreparedToolCall,
        observed_at: peritus_policy::AuthorityInstant,
    ) -> Result<(ReplayReservationOwner, u64), RouterError> {
        if prepared.call().limits().progress_contract() != ProgressContract::PagedV2 {
            return Err(RouterError::new(
                RouterErrorKind::Durability,
                "adopt durable tool execution",
                "active owner adoption requires durable paged progress",
            ));
        }
        let action_id = prepared.call().action_id();
        let record = match self.entries.get(&action_id).cloned() {
            Some(record) => record,
            None => self
                .store
                .lookup(action_id)
                .map_err(|_| durability_error())?
                .ok_or_else(durability_error)?,
        };
        if record.action_id != action_id
            || record.replay_identity != prepared.replay_identity().digest()
            || !matches!(record.kind, ReplayRecordKind::Reserved | ReplayRecordKind::Active)
        {
            return Err(RouterError::new(
                RouterErrorKind::ReplayConflict,
                "adopt durable tool execution",
                "durable replay receipt does not identify this active prepared call",
            ));
        }
        let reservation_owner = record.reservation_owner.ok_or_else(durability_error)?;
        let progress = self
            .store
            .latest_progress(action_id)
            .map_err(|_| durability_error())?;
        let next_sequence = match progress {
            Some(page)
                if page.replay_identity == prepared.replay_identity().digest()
                    && page.prepared_digest == prepared.prepared_digest() =>
            {
                if let Some(last) = page.events.last()
                    && (last.observed_at().epoch() != observed_at.epoch()
                        || last.observed_at().tick_millis() > observed_at.tick_millis())
                {
                    return Err(RouterError::new(
                        RouterErrorKind::InvalidObservation,
                        "adopt durable tool execution",
                        "adoption time regresses or crosses the durable progress epoch",
                    ));
                }
                page.end
            }
            Some(_) => {
                return Err(RouterError::new(
                    RouterErrorKind::ReplayConflict,
                    "adopt durable tool execution",
                    "durable progress belongs to another prepared invocation",
                ));
            }
            None => 0,
        };
        Ok((reservation_owner, next_sequence))
    }

    pub(crate) fn append_progress(
        &mut self,
        prepared: &PreparedToolCall,
        start: u64,
        events: &[ToolProgress],
    ) -> Result<Option<ProgressPage>, RouterError> {
        if events.is_empty() {
            return Ok(None);
        }
        if let Some(stored) = self
            .store
            .progress_page(prepared.call().action_id(), start)
            .map_err(|_| durability_error())?
        {
            let candidate = ProgressPage::new(
                prepared,
                start,
                stored.previous_digest,
                events.to_vec(),
            )
            .map_err(|_| durability_error())?;
            return if stored == candidate {
                Ok(Some(stored))
            } else {
                Err(RouterError::new(
                    RouterErrorKind::ReplayConflict,
                    "append tool progress page",
                    "progress frontier is already bound to different event bytes",
                ))
            };
        }
        let previous = self
            .store
            .latest_progress(prepared.call().action_id())
            .map_err(|_| durability_error())?;
        if previous.as_ref().is_some_and(|page| {
            page.replay_identity != prepared.replay_identity().digest()
                || page.prepared_digest != prepared.prepared_digest()
                || page.end != start
        }) || previous.is_none() && start != 0
        {
            return Err(RouterError::new(
                RouterErrorKind::ReplayConflict,
                "append tool progress page",
                "progress page does not continue the durable invocation frontier",
            ));
        }
        let page = ProgressPage::new(
            prepared,
            start,
            previous.as_ref().map(ProgressPage::digest),
            events.to_vec(),
        )
        .map_err(|_| durability_error())?;
        match self.store.append_progress(&page) {
            Ok(ReplayAppendOutcome::Appended | ReplayAppendOutcome::AlreadyPresent) => {
                Ok(Some(page))
            }
            Err(error) => match self
                .store
                .progress_page(prepared.call().action_id(), start)
            {
                Ok(Some(stored)) if stored == page => Ok(Some(stored)),
                Ok(Some(_)) => Err(RouterError::new(
                    RouterErrorKind::ReplayConflict,
                    "append tool progress page",
                    "ambiguous page append resolved to different durable bytes",
                )),
                Ok(None) | Err(_) => Err(if error.kind() == ReplayStoreErrorKind::Occupied {
                    RouterError::new(
                        RouterErrorKind::ReplayConflict,
                        "append tool progress page",
                        "progress page frontier is already occupied",
                    )
                } else {
                    durability_error()
                }),
            },
        }
    }

    pub(crate) fn progress_page(
        &mut self,
        action_id: ActionId,
        replay_identity: Sha256Digest,
        start: u64,
    ) -> Result<Option<ProgressPage>, RouterError> {
        let page = self
            .store
            .progress_page(action_id, start)
            .map_err(|_| durability_error())?;
        if page.as_ref().is_some_and(|page| page.replay_identity != replay_identity) {
            return Err(RouterError::new(
                RouterErrorKind::ReplayConflict,
                "read tool progress page",
                "progress cursor replay identity differs",
            ));
        }
        Ok(page)
    }

    pub(crate) fn reserve(&mut self, prepared: &PreparedToolCall) -> Result<(), RouterError> {
        let action_id = prepared.call().action_id();
        match self.entries.entry(action_id) {
            Entry::Occupied(_) => Err(RouterError::new(
                RouterErrorKind::ReplayConflict,
                "reserve tool invocation",
                "action is already owned by this router",
            )),
            Entry::Vacant(slot) => {
                let mut attempt = ReplayReservationAttempt::new(
                    action_id,
                    prepared.replay_identity().digest(),
                )
                .map_err(|error| reservation_error(&error))?;
                let reservation = reserve_replay(self.store.as_mut(), &mut attempt)
                    .map_err(|error| reservation_error(&error))?;
                slot.insert(*reservation.record);
                Ok(())
            }
        }
    }

    pub(crate) fn adopt(
        &mut self,
        prepared: &PreparedToolCall,
        reservation: &ReplayReservation,
    ) -> Result<(), RouterError> {
        let record = reservation.record.as_ref();
        if record.kind != ReplayRecordKind::Reserved
            || record.action_id != prepared.call().action_id()
            || record.replay_identity != prepared.replay_identity().digest()
            || record.reservation_owner.is_none()
        {
            return Err(RouterError::new(
                RouterErrorKind::ReplayConflict,
                "adopt durable tool reservation",
                "durable reservation differs from the prepared call",
            ));
        }
        match self.entries.entry(record.action_id) {
            Entry::Occupied(_) => Err(RouterError::new(
                RouterErrorKind::ReplayConflict,
                "adopt durable tool reservation",
                "action is already owned by this router",
            )),
            Entry::Vacant(slot) => {
                slot.insert(record.clone());
                Ok(())
            }
        }
    }

    pub(crate) fn pending_publication(
        &self,
        action_id: ActionId,
    ) -> Option<PendingReplayReceipt> {
        self.entries
            .get(&action_id)
            .filter(|record| is_settled(record.kind))
            .cloned()
            .map(|record| PendingReplayReceipt { record: Box::new(record) })
    }

    pub(crate) fn acknowledge_publication(
        &mut self,
        published: PublishedReplayReceipt,
    ) -> Result<Option<(ActionId, Sha256Digest)>, RouterError> {
        let PublishedReplayReceipt { record } = published;
        let record = record.as_ref();
        let Some(retained) = self.entries.get(&record.action_id) else {
            return Ok(None);
        };
        if retained != record || !is_settled(retained.kind) {
            return Err(RouterError::new(
                RouterErrorKind::ReplayConflict,
                "acknowledge replay receipt publication",
                "published receipt differs from the router-owned pending receipt",
            ));
        }
        self.entries.remove(&record.action_id);
        Ok(Some((record.action_id, record.replay_identity)))
    }

    pub(crate) fn reservation_owner(
        &self,
        prepared: &PreparedToolCall,
    ) -> Result<ReplayReservationOwner, RouterError> {
        self.retained_reservation_owner(prepared)?.ok_or_else(durability_error)
    }

    pub(crate) fn retained_reservation_owner(
        &self,
        prepared: &PreparedToolCall,
    ) -> Result<Option<ReplayReservationOwner>, RouterError> {
        let Some(record) = self.entries.get(&prepared.call().action_id()) else {
            return Ok(None);
        };
        if record.replay_identity != prepared.replay_identity().digest() {
            return Err(RouterError::new(
                RouterErrorKind::ReplayConflict,
                "reconcile interrupted tool dispatch",
                "retained replay identity differs from the prepared call",
            ));
        }
        record.reservation_owner.map(Some).ok_or_else(durability_error)
    }

    pub(crate) fn mark_active(
        &mut self,
        prepared: &PreparedToolCall,
        reservation_owner: ReplayReservationOwner,
    ) {
        let record = ReplayRecord::active(prepared, reservation_owner);
        self.entries.insert(prepared.call().action_id(), record.clone());
        // The durable reservation already prevents a second effect. Retain live ownership and let
        // terminal publication advance directly from Reserved if this advisory phase write fails.
        let _ = self.store.append(&record);
    }

    pub(crate) fn complete(
        &mut self,
        prepared: &PreparedToolCall,
        reservation_owner: ReplayReservationOwner,
        result: ToolResult,
    ) {
        let action_id = prepared.call().action_id();
        let record = ReplayRecord::terminal(prepared, reservation_owner, result);
        self.entries.insert(action_id, record.clone());
        if self.store.append(&record).is_ok() {
            self.entries.remove(&action_id);
        }
    }

    pub(crate) fn indeterminate(
        &mut self,
        prepared: &PreparedToolCall,
        reservation_owner: ReplayReservationOwner,
    ) {
        let action_id = prepared.call().action_id();
        let record = ReplayRecord::indeterminate(prepared, reservation_owner);
        self.entries.insert(action_id, record.clone());
        if self.store.append(&record).is_ok() {
            self.entries.remove(&action_id);
        }
    }

    fn publish_settled(&mut self, record: &ReplayRecord) {
        if is_settled(record.kind) && self.store.append(record).is_ok() {
            self.entries.remove(&record.action_id);
        }
    }
}

fn validate_transition(
    previous: &ReplayRecord,
    next: &ReplayRecord,
) -> Result<(), ReplayStoreError> {
    if previous.action_id != next.action_id
        || previous.replay_identity != next.replay_identity
        || previous.reservation_owner != next.reservation_owner
    {
        return Err(ReplayStoreError::integrity(
            "replay receipt transition changes its action, replay identity, or reservation owner",
        ));
    }
    if previous == next {
        return Ok(());
    }
    let legal = matches!(
        (previous.kind, next.kind),
        (
            ReplayRecordKind::Reserved,
            ReplayRecordKind::Active
                | ReplayRecordKind::NonIdempotentTerminal
                | ReplayRecordKind::ReplayTerminal
                | ReplayRecordKind::Indeterminate
        ) | (
            ReplayRecordKind::Active,
            ReplayRecordKind::NonIdempotentTerminal
                | ReplayRecordKind::ReplayTerminal
                | ReplayRecordKind::Indeterminate
        )
    );
    if !legal {
        return Err(ReplayStoreError::integrity(
            "replay receipt transition regresses or changes a settled outcome",
        ));
    }
    Ok(())
}

const fn is_settled(kind: ReplayRecordKind) -> bool {
    matches!(
        kind,
        ReplayRecordKind::NonIdempotentTerminal
            | ReplayRecordKind::ReplayTerminal
            | ReplayRecordKind::Indeterminate
    )
}

const fn durability_error() -> RouterError {
    RouterError::new(
        RouterErrorKind::Durability,
        "access durable tool replay history",
        "durable replay history is unavailable or inconsistent",
    )
}

fn reservation_error(error: &ReplayStoreError) -> RouterError {
    if error.kind() == ReplayStoreErrorKind::Occupied {
        RouterError::new(
            RouterErrorKind::ReplayConflict,
            "reserve tool invocation",
            "action identity already has a durable reservation",
        )
    } else {
        durability_error()
    }
}
