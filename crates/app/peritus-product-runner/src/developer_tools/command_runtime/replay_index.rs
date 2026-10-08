//! Append-only durable replay receipts for the command router.

use std::{
    cmp,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use peritus_provider_core::CancellationToken;
use peritus_policy::AuthorityInstant;
use peritus_tool_router::{
    PendingReplayReceipt, ProgressPage, PublishedReplayReceipt, ReplayAppendOutcome, ReplayRecord,
    ReplayRecordKind, ReplayReservation, ReplayReservationAttempt, ReplayReservationOwner,
    ReplayStore, ReplayStoreError, ReplayStoreErrorKind, publish_replay_receipt, reserve_replay,
};
use peritus_types::{ActionId, Sha256Digest};
use rusqlite::{Connection, TransactionBehavior, params};

const DATABASE: &str = "command-replay.sqlite3";
const RETRY_MINIMUM: Duration = Duration::from_millis(5);
const RETRY_MAXIMUM: Duration = Duration::from_millis(100);
const CONTENTION_REPORT_INTERVAL: Duration = Duration::from_secs(5);
const RECORD_PAGE_SIZE: usize = 256;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS command_replay_events_v1 (
    action_id BLOB NOT NULL CHECK(length(action_id) = 16),
    sequence INTEGER NOT NULL CHECK(sequence > 0),
    replay_identity BLOB NOT NULL CHECK(length(replay_identity) = 32),
    reservation_owner BLOB CHECK(reservation_owner IS NULL OR length(reservation_owner) = 32),
    phase INTEGER NOT NULL CHECK(phase BETWEEN 1 AND 5),
    terminal BLOB,
    record_digest BLOB NOT NULL CHECK(length(record_digest) = 32),
    PRIMARY KEY(action_id, sequence),
    CHECK(
        (phase IN (1, 2, 5) AND terminal IS NULL)
        OR (phase IN (3, 4) AND terminal IS NOT NULL AND length(terminal) > 0)
    )
) STRICT, WITHOUT ROWID;
CREATE TRIGGER IF NOT EXISTS command_replay_events_v1_no_update
BEFORE UPDATE ON command_replay_events_v1
BEGIN
    SELECT RAISE(ABORT, 'command replay receipts are immutable');
END;
CREATE TRIGGER IF NOT EXISTS command_replay_events_v1_no_delete
BEFORE DELETE ON command_replay_events_v1
BEGIN
    SELECT RAISE(ABORT, 'command replay receipts are immutable');
END;
CREATE TABLE IF NOT EXISTS command_progress_pages_v1 (
    action_id BLOB NOT NULL CHECK(length(action_id) = 16),
    start_frontier BLOB NOT NULL CHECK(length(start_frontier) = 8),
    end_frontier BLOB NOT NULL CHECK(length(end_frontier) = 8),
    replay_identity BLOB NOT NULL CHECK(length(replay_identity) = 32),
    prepared_digest BLOB NOT NULL CHECK(length(prepared_digest) = 32),
    previous_digest BLOB CHECK(previous_digest IS NULL OR length(previous_digest) = 32),
    page_digest BLOB NOT NULL CHECK(length(page_digest) = 32),
    events BLOB NOT NULL,
    PRIMARY KEY(action_id, start_frontier)
) STRICT, WITHOUT ROWID;
CREATE TRIGGER IF NOT EXISTS command_progress_pages_v1_no_update
BEFORE UPDATE ON command_progress_pages_v1
BEGIN
    SELECT RAISE(ABORT, 'command progress pages are immutable');
END;
CREATE TRIGGER IF NOT EXISTS command_progress_pages_v1_no_delete
BEFORE DELETE ON command_progress_pages_v1
BEGIN
    SELECT RAISE(ABORT, 'command progress pages are immutable');
END;
";

/// Path-backed store; each operation opens a short-lived connection so router ownership remains
/// movable across runtime worker threads.
#[derive(Clone)]
pub(super) struct ReplayIndex {
    path: PathBuf,
}

#[derive(Clone)]
pub(super) struct RecoveryProgress {
    latest: Option<ProgressPage>,
    observed_at: AuthorityInstant,
    process_cursor: u64,
    truncated: bool,
}

impl RecoveryProgress {
    pub(super) fn latest(&self) -> Option<&ProgressPage> { self.latest.as_ref() }

    pub(super) const fn observed_at(&self) -> AuthorityInstant { self.observed_at }

    pub(super) const fn process_cursor(&self) -> u64 { self.process_cursor }

    pub(super) const fn truncated(&self) -> bool { self.truncated }

    pub(super) fn next_frontier(&self) -> u64 {
        self.latest.as_ref().map_or(0, ProgressPage::end)
    }
}

impl ReplayIndex {
    pub(super) fn open(root: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(root)
            .map_err(|error| format!("create command replay root: {error}"))?;
        let path = root.join(DATABASE);
        let connection = connect(&path).map_err(|error| error.to_string())?;
        connection
            .execute_batch(SCHEMA)
            .map_err(|error| format!("initialize command replay index: {error}"))?;
        ensure_reservation_owner_column(&connection)
            .map_err(|error| format!("migrate command replay owner: {error}"))?;
        Ok(Self { path })
    }

    pub(super) fn reserve_cancellable(
        &self,
        action_id: ActionId,
        replay_identity: Sha256Digest,
        cancellation: &CancellationToken,
    ) -> Result<ReplayReservation, OperationError> {
        let mut attempt = ReplayReservationAttempt::new(action_id, replay_identity)
            .map_err(OperationError::Store)?;
        let mut contention = Contention::new();
        loop {
            if cancellation.is_cancelled() {
                return Err(OperationError::Cancelled);
            }
            let mut store = self.clone();
            match reserve_replay(&mut store, &mut attempt) {
                Ok(reservation) => return Ok(reservation),
                Err(error)
                    if matches!(
                        error.kind(),
                        ReplayStoreErrorKind::Contended
                            | ReplayStoreErrorKind::AmbiguousCommit
                    ) =>
                {
                    contention.wait("durable replay reservation", cancellation)?;
                }
                Err(error) => return Err(OperationError::Store(error)),
            }
        }
    }

    pub(super) fn publish_cancellable(
        &self,
        pending: &PendingReplayReceipt,
        cancellation: &CancellationToken,
    ) -> Result<PublishedReplayReceipt, OperationError> {
        let mut contention = Contention::new();
        loop {
            if cancellation.is_cancelled() {
                return Err(OperationError::Cancelled);
            }
            let mut store = self.clone();
            match publish_replay_receipt(&mut store, pending) {
                Ok(published) => return Ok(published),
                Err(error)
                    if matches!(
                        error.kind(),
                        ReplayStoreErrorKind::Contended
                            | ReplayStoreErrorKind::AmbiguousCommit
                    ) =>
                {
                    contention.wait("durable replay receipt publication", cancellation)?;
                }
                Err(error) => return Err(OperationError::Store(error)),
            }
        }
    }

    /// Reads the next physical page of latest authoritative replay receipts in action-ID order.
    pub(super) fn records_page(
        &self,
        after: Option<ActionId>,
    ) -> Result<Vec<ReplayRecord>, ReplayStoreError> {
        let connection = connect(&self.path)?;
        let mut statement = connection
            .prepare(
                "SELECT action_id FROM command_replay_events_v1
                 WHERE (?1 IS NULL OR action_id > ?1)
                 GROUP BY action_id ORDER BY action_id LIMIT ?2",
            )
            .map_err(|error| storage("prepare command replay receipt page", error))?;
        let limit = i64::try_from(RECORD_PAGE_SIZE).map_err(|_| {
            ReplayStoreError::integrity("command replay page size is not representable")
        })?;
        let after = after.map(|action_id| action_id.as_bytes().to_vec());
        let mut rows = statement
            .query(params![after, limit])
            .map_err(|error| storage("read command replay receipt page", error))?;
        let mut action_ids = Vec::with_capacity(RECORD_PAGE_SIZE);
        while let Some(row) = rows
            .next()
            .map_err(|error| storage("advance command replay receipt page", error))?
        {
            let bytes: Vec<u8> = row
                .get(0)
                .map_err(|error| storage("decode command replay page identity", error))?;
            let bytes: [u8; 16] = bytes.try_into().map_err(|_| {
                ReplayStoreError::integrity("command replay page identity has an invalid width")
            })?;
            action_ids.push(ActionId::new(bytes).map_err(|_| {
                ReplayStoreError::integrity("command replay page identity is zero")
            })?);
        }
        drop(rows);
        drop(statement);
        action_ids
            .into_iter()
            .map(|action_id| {
                load(&connection, action_id)?.map(|stored| stored.record).ok_or_else(|| {
                    ReplayStoreError::integrity(
                        "command replay page identity has no receipt history",
                    )
                })
            })
            .collect()
    }

    pub(super) fn record(
        &self,
        action_id: ActionId,
    ) -> Result<Option<ReplayRecord>, ReplayStoreError> {
        let connection = connect(&self.path)?;
        load(&connection, action_id).map(|stored| stored.map(|stored| stored.record))
    }

    /// Replays the complete immutable progress-page chain without retaining its lifetime events.
    pub(super) fn recovery_progress(
        &self,
        action_id: ActionId,
    ) -> Result<RecoveryProgress, ReplayStoreError> {
        let connection = connect(&self.path)?;
        let mut start = 0_u64;
        let mut previous = None;
        let mut latest = None;
        let mut observed_at = AuthorityInstant::new(peritus_types::Generation::first(), 20);
        let mut process_cursor = 0_u64;
        let mut truncated = false;
        loop {
            let Some(page) = load_progress(&connection, action_id, Some(start))? else { break };
            if page.start() != start || page.previous_digest() != previous {
                return Err(ReplayStoreError::integrity(
                    "command progress recovery encountered a discontinuous page chain",
                ));
            }
            for event in page.events() {
                observed_at = event.observed_at();
                match event.structured() {
                    None if event.sequence() == 0 => {}
                    None => {
                        return Err(ReplayStoreError::integrity(
                            "command progress event has no durable process cursor",
                        ));
                    }
                    Some(structured) => {
                        let value = serde_json::from_slice::<serde_json::Value>(
                            structured.canonical_bytes(),
                        )
                        .map_err(|_| {
                            ReplayStoreError::integrity(
                                "command progress process cursor is malformed",
                            )
                        })?;
                        let sequence = value
                            .get("process_sequence")
                            .and_then(serde_json::Value::as_str)
                            .and_then(|value| value.parse::<u64>().ok())
                            .filter(|sequence| *sequence > process_cursor)
                            .ok_or_else(|| {
                                ReplayStoreError::integrity(
                                    "command progress process cursor did not advance",
                                )
                            })?;
                        if process_cursor.checked_add(1) != Some(sequence) {
                            truncated = true;
                        }
                        process_cursor = sequence;
                        if value
                            .get("event_loss_through")
                            .is_some_and(|value| !value.is_null())
                        {
                            truncated = true;
                        }
                    }
                }
            }
            start = page.end();
            previous = Some(page.digest());
            latest = Some(page);
        }
        Ok(RecoveryProgress { latest, observed_at, process_cursor, truncated })
    }
}

pub(super) enum OperationError {
    Cancelled,
    Store(ReplayStoreError),
}

impl ReplayStore for ReplayIndex {
    fn lookup(
        &mut self,
        action_id: ActionId,
    ) -> Result<Option<ReplayRecord>, ReplayStoreError> {
        let connection = connect(&self.path)?;
        load(&connection, action_id).map(|value| value.map(|stored| stored.record))
    }

    fn append(
        &mut self,
        record: &ReplayRecord,
    ) -> Result<ReplayAppendOutcome, ReplayStoreError> {
        if record.kind() == ReplayRecordKind::ReplayTerminal {
            return Err(ReplayStoreError::integrity(
                "command replay index cannot reconstruct replayable typed terminal envelopes",
            ));
        }
        let mut connection = connect(&self.path)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| storage("begin command replay receipt", error))?;
        let previous = load(&transaction, record.action_id())?;
        let sequence = if let Some(previous) = previous {
            if previous.record == *record {
                return Ok(ReplayAppendOutcome::AlreadyPresent);
            }
            if record.kind() == ReplayRecordKind::Reserved {
                return Err(ReplayStoreError::occupied(
                    "command action identity is already reserved by another owner",
                ));
            }
            validate_advance(&previous.record, record)?;
            previous
                .sequence
                .checked_add(1)
                .ok_or_else(|| ReplayStoreError::integrity("command replay sequence overflowed"))?
        } else {
            if record.kind() != ReplayRecordKind::Reserved {
                return Err(ReplayStoreError::integrity(
                    "command replay history does not begin with a reservation",
                ));
            }
            1
        };
        let phase = phase_tag(record.kind());
        let terminal = record.terminal_bytes();
        let reservation_owner = record
            .reservation_owner()
            .map(|owner| owner.as_bytes().to_vec());
        let digest = receipt_digest(
            record.action_id(),
            sequence,
            record.replay_identity(),
            record.reservation_owner(),
            phase,
            terminal,
        );
        transaction
            .execute(
                "INSERT INTO command_replay_events_v1(
                    action_id, sequence, replay_identity, reservation_owner, phase, terminal,
                    record_digest
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    record.action_id().as_bytes().as_slice(),
                    sequence,
                    record.replay_identity().as_bytes().as_slice(),
                    reservation_owner,
                    phase,
                    terminal,
                    digest.as_bytes().as_slice(),
                ],
            )
            .map_err(|error| storage("append command replay receipt", error))?;
        transaction
            .commit()
            .map_err(|error| {
                ReplayStoreError::ambiguous_commit(format!(
                    "commit command replay receipt: {error}"
                ))
            })?;
        Ok(ReplayAppendOutcome::Appended)
    }

    fn latest_progress(
        &mut self,
        action_id: ActionId,
    ) -> Result<Option<ProgressPage>, ReplayStoreError> {
        let connection = connect(&self.path)?;
        load_progress(&connection, action_id, None)
    }

    fn progress_page(
        &mut self,
        action_id: ActionId,
        start: u64,
    ) -> Result<Option<ProgressPage>, ReplayStoreError> {
        let connection = connect(&self.path)?;
        load_progress(&connection, action_id, Some(start))
    }

    fn append_progress(
        &mut self,
        page: &ProgressPage,
    ) -> Result<ReplayAppendOutcome, ReplayStoreError> {
        let mut connection = connect(&self.path)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| storage("begin command progress page", error))?;
        if let Some(stored) = load_progress(&transaction, page.action_id(), Some(page.start()))? {
            return if stored == *page {
                Ok(ReplayAppendOutcome::AlreadyPresent)
            } else {
                Err(ReplayStoreError::integrity(
                    "command progress frontier is already bound to different bytes",
                ))
            };
        }
        let replay = load(&transaction, page.action_id())?
            .ok_or_else(|| ReplayStoreError::integrity("progress page has no replay reservation"))?;
        if replay.record.replay_identity() != page.replay_identity() {
            return Err(ReplayStoreError::integrity(
                "progress page replay identity differs from its reservation",
            ));
        }
        let previous = load_progress(&transaction, page.action_id(), None)?;
        let valid = match previous {
            Some(previous) => {
                previous.replay_identity() == page.replay_identity()
                    && previous.prepared_digest() == page.prepared_digest()
                    && previous.end() == page.start()
                    && page.previous_digest() == Some(previous.digest())
            }
            None => page.start() == 0 && page.previous_digest().is_none(),
        };
        if !valid {
            return Err(ReplayStoreError::integrity(
                "command progress page breaks its durable digest chain",
            ));
        }
        transaction
            .execute(
                "INSERT INTO command_progress_pages_v1(
                    action_id, start_frontier, end_frontier, replay_identity, prepared_digest,
                    previous_digest, page_digest, events
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    page.action_id().as_bytes().as_slice(),
                    page.start().to_be_bytes().as_slice(),
                    page.end().to_be_bytes().as_slice(),
                    page.replay_identity().as_bytes().as_slice(),
                    page.prepared_digest().as_bytes().as_slice(),
                    page.previous_digest().map(|digest| digest.as_bytes().to_vec()),
                    page.digest().as_bytes().as_slice(),
                    encode_progress_events(page.canonical_events())?,
                ],
            )
            .map_err(|error| storage("append command progress page", error))?;
        transaction.commit().map_err(|error| {
            ReplayStoreError::ambiguous_commit(format!("commit command progress page: {error}"))
        })?;
        Ok(ReplayAppendOutcome::Appended)
    }
}

struct StoredRecord {
    sequence: i64,
    record: ReplayRecord,
}

fn connect(path: &Path) -> Result<Connection, ReplayStoreError> {
    let connection = Connection::open(path)
        .map_err(|error| storage("open command replay index", error))?;
    connection
        .busy_timeout(Duration::ZERO)
        .map_err(|error| storage("configure command replay contention", error))?;
    connection
        .pragma_update(None, "synchronous", "EXTRA")
        .map_err(|error| storage("configure command replay durability", error))?;
    Ok(connection)
}

fn ensure_reservation_owner_column(connection: &Connection) -> Result<(), ReplayStoreError> {
    if reservation_owner_column_exists(connection)? {
        return Ok(());
    }
    match connection.execute(
        "ALTER TABLE command_replay_events_v1
         ADD COLUMN reservation_owner BLOB
         CHECK(reservation_owner IS NULL OR length(reservation_owner) = 32)",
        [],
    ) {
        Ok(_) => Ok(()),
        Err(_) if reservation_owner_column_exists(connection)? => Ok(()),
        Err(error) => Err(storage("add command replay reservation owner", error)),
    }
}

fn reservation_owner_column_exists(
    connection: &Connection,
) -> Result<bool, ReplayStoreError> {
    let mut statement = connection
        .prepare("PRAGMA table_info(command_replay_events_v1)")
        .map_err(|error| storage("inspect command replay schema", error))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| storage("read command replay schema", error))?;
    for column in columns {
        if column.map_err(|error| storage("decode command replay schema", error))?
            == "reservation_owner"
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn load(
    connection: &Connection,
    action_id: ActionId,
) -> Result<Option<StoredRecord>, ReplayStoreError> {
    let mut statement = connection
        .prepare(
            "SELECT sequence, replay_identity, reservation_owner, phase, terminal, record_digest
             FROM command_replay_events_v1
             WHERE action_id = ?1
             ORDER BY sequence ASC",
        )
        .map_err(|error| storage("prepare command replay receipt read", error))?;
    let rows = statement
        .query_map(params![action_id.as_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Option<Vec<u8>>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<Vec<u8>>>(4)?,
                row.get::<_, Vec<u8>>(5)?,
            ))
        })
        .map_err(|error| storage("read command replay receipt history", error))?;
    let mut latest: Option<StoredRecord> = None;
    for row in rows {
        let stored = decode(
            action_id,
            row.map_err(|error| storage("decode command replay receipt row", error))?,
        )?;
        if let Some(previous) = &latest {
            if previous.sequence.checked_add(1) != Some(stored.sequence) {
                return Err(ReplayStoreError::integrity(
                    "command replay receipt history has a sequence gap",
                ));
            }
            validate_advance(&previous.record, &stored.record)?;
        } else if stored.sequence != 1 || stored.record.kind() != ReplayRecordKind::Reserved {
            return Err(ReplayStoreError::integrity(
                "command replay receipt history does not begin with sequence-one reservation",
            ));
        }
        latest = Some(stored);
    }
    Ok(latest)
}

fn load_progress(
    connection: &Connection,
    action_id: ActionId,
    start: Option<u64>,
) -> Result<Option<ProgressPage>, ReplayStoreError> {
    let (sql, start_bytes) = match start {
        Some(start) => (
            "SELECT start_frontier, end_frontier, replay_identity, prepared_digest,
                    previous_digest, page_digest, events
             FROM command_progress_pages_v1
             WHERE action_id = ?1 AND start_frontier = ?2",
            Some(start.to_be_bytes()),
        ),
        None => (
            "SELECT start_frontier, end_frontier, replay_identity, prepared_digest,
                    previous_digest, page_digest, events
             FROM command_progress_pages_v1
             WHERE action_id = ?1
             ORDER BY start_frontier DESC LIMIT 1",
            None,
        ),
    };
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| storage("prepare command progress page read", error))?;
    let mut rows = if let Some(start) = start_bytes.as_ref() {
        statement.query(params![action_id.as_bytes().as_slice(), start.as_slice()])
    } else {
        statement.query(params![action_id.as_bytes().as_slice()])
    }
    .map_err(|error| storage("read command progress page", error))?;
    let Some(row) = rows
        .next()
        .map_err(|error| storage("advance command progress page read", error))?
    else {
        return Ok(None);
    };
    let stored = (
        row.get::<_, Vec<u8>>(0),
        row.get::<_, Vec<u8>>(1),
        row.get::<_, Vec<u8>>(2),
        row.get::<_, Vec<u8>>(3),
        row.get::<_, Option<Vec<u8>>>(4),
        row.get::<_, Vec<u8>>(5),
        row.get::<_, Vec<u8>>(6),
    );
    let stored = (
        stored.0.map_err(|error| storage("decode progress start", error))?,
        stored.1.map_err(|error| storage("decode progress end", error))?,
        stored.2.map_err(|error| storage("decode progress replay identity", error))?,
        stored.3.map_err(|error| storage("decode progress prepared digest", error))?,
        stored.4.map_err(|error| storage("decode progress previous digest", error))?,
        stored.5.map_err(|error| storage("decode progress page digest", error))?,
        stored.6.map_err(|error| storage("decode progress events", error))?,
    );
    if rows
        .next()
        .map_err(|error| storage("check command progress page uniqueness", error))?
        .is_some()
    {
        return Err(ReplayStoreError::integrity(
            "command progress query returned more than one page",
        ));
    }
    decode_progress(action_id, stored).map(Some)
}

type StoredProgressFields = (
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Option<Vec<u8>>,
    Vec<u8>,
    Vec<u8>,
);

fn decode_progress(
    action_id: ActionId,
    fields: StoredProgressFields,
) -> Result<ProgressPage, ReplayStoreError> {
    let (start, end, replay_identity, prepared_digest, previous_digest, digest, events) = fields;
    let start = u64::from_be_bytes(start.try_into().map_err(|_| {
        ReplayStoreError::integrity("command progress start has an invalid width")
    })?);
    let end = u64::from_be_bytes(end.try_into().map_err(|_| {
        ReplayStoreError::integrity("command progress end has an invalid width")
    })?);
    let replay_identity = Sha256Digest::new(replay_identity.try_into().map_err(|_| {
        ReplayStoreError::integrity("command progress replay identity has an invalid width")
    })?);
    let prepared_digest = Sha256Digest::new(prepared_digest.try_into().map_err(|_| {
        ReplayStoreError::integrity("command progress prepared digest has an invalid width")
    })?);
    let previous_digest = previous_digest
        .map(|value| {
            value.try_into().map(Sha256Digest::new).map_err(|_| {
                ReplayStoreError::integrity("command progress previous digest has an invalid width")
            })
        })
        .transpose()?;
    let digest = Sha256Digest::new(digest.try_into().map_err(|_| {
        ReplayStoreError::integrity("command progress page digest has an invalid width")
    })?);
    ProgressPage::recovered(
        action_id,
        replay_identity,
        prepared_digest,
        start,
        end,
        previous_digest,
        digest,
        decode_progress_events(&events)?,
    )
}

fn encode_progress_events(events: &[Vec<u8>]) -> Result<Vec<u8>, ReplayStoreError> {
    let mut encoded = Vec::new();
    encoded.extend_from_slice(
        &u64::try_from(events.len())
            .map_err(|_| ReplayStoreError::integrity("progress event count is not representable"))?
            .to_be_bytes(),
    );
    for event in events {
        encoded.extend_from_slice(
            &u64::try_from(event.len())
                .map_err(|_| ReplayStoreError::integrity("progress event length is not representable"))?
                .to_be_bytes(),
        );
        encoded.extend_from_slice(event);
    }
    Ok(encoded)
}

fn decode_progress_events(mut encoded: &[u8]) -> Result<Vec<Vec<u8>>, ReplayStoreError> {
    let count = take_u64(&mut encoded)?;
    if count > (encoded.len() / 8) as u64 {
        return Err(ReplayStoreError::integrity(
            "progress event count exceeds the stored frame",
        ));
    }
    let capacity = usize::try_from(count)
        .map_err(|_| ReplayStoreError::integrity("progress event count exceeds this host"))?;
    let mut events = Vec::with_capacity(capacity);
    for _ in 0..count {
        let length = usize::try_from(take_u64(&mut encoded)?)
            .map_err(|_| ReplayStoreError::integrity("progress event length exceeds this host"))?;
        if encoded.len() < length {
            return Err(ReplayStoreError::integrity("progress event frame is truncated"));
        }
        let (event, rest) = encoded.split_at(length);
        events.push(event.to_vec());
        encoded = rest;
    }
    if !encoded.is_empty() {
        return Err(ReplayStoreError::integrity("progress event frame has trailing bytes"));
    }
    Ok(events)
}

fn take_u64(input: &mut &[u8]) -> Result<u64, ReplayStoreError> {
    if input.len() < 8 {
        return Err(ReplayStoreError::integrity("progress event frame is truncated"));
    }
    let (value, rest) = input.split_at(8);
    *input = rest;
    Ok(u64::from_be_bytes(value.try_into().expect("eight-byte slice")))
}

fn decode(
    action_id: ActionId,
    row: (i64, Vec<u8>, Option<Vec<u8>>, i64, Option<Vec<u8>>, Vec<u8>),
) -> Result<StoredRecord, ReplayStoreError> {
    let (sequence, replay_identity, reservation_owner, phase, terminal, stored_digest) = row;
    if sequence <= 0 {
        return Err(ReplayStoreError::integrity(
            "command replay receipt has a non-positive sequence",
        ));
    }
    let replay_identity: [u8; 32] = replay_identity.try_into().map_err(|_| {
        ReplayStoreError::integrity("command replay receipt identity has an invalid width")
    })?;
    let stored_digest: [u8; 32] = stored_digest.try_into().map_err(|_| {
        ReplayStoreError::integrity("command replay receipt digest has an invalid width")
    })?;
    let replay_identity = Sha256Digest::new(replay_identity);
    let reservation_owner = reservation_owner
        .map(|owner| {
            owner.try_into().map(ReplayReservationOwner::new).map_err(|_| {
                ReplayStoreError::integrity(
                    "command replay reservation owner has an invalid width",
                )
            })
        })
        .transpose()?;
    let expected = receipt_digest(
        action_id,
        sequence,
        replay_identity,
        reservation_owner,
        phase,
        terminal.as_deref(),
    );
    if expected != Sha256Digest::new(stored_digest) {
        return Err(ReplayStoreError::integrity(
            "command replay receipt digest does not match its stored fields",
        ));
    }
    let kind = match phase {
        1 => ReplayRecordKind::Reserved,
        2 => ReplayRecordKind::Active,
        3 => ReplayRecordKind::NonIdempotentTerminal,
        4 => {
            return Err(ReplayStoreError::integrity(
                "command replay index contains an unsupported replayable terminal receipt",
            ));
        }
        5 => ReplayRecordKind::Indeterminate,
        _ => return Err(ReplayStoreError::integrity("command replay receipt phase is invalid")),
    };
    let record = ReplayRecord::recovered(
        action_id,
        replay_identity,
        reservation_owner,
        kind,
        terminal,
    )?;
    Ok(StoredRecord { sequence, record })
}

fn validate_advance(
    previous: &ReplayRecord,
    next: &ReplayRecord,
) -> Result<(), ReplayStoreError> {
    if previous.action_id() != next.action_id()
        || previous.replay_identity() != next.replay_identity()
        || previous.reservation_owner() != next.reservation_owner()
    {
        return Err(ReplayStoreError::integrity(
            "command replay transition changes its identity or reservation owner",
        ));
    }
    if previous == next {
        return Ok(());
    }
    let legal = matches!(
        (previous.kind(), next.kind()),
        (
            ReplayRecordKind::Reserved,
            ReplayRecordKind::Active
                | ReplayRecordKind::NonIdempotentTerminal
                | ReplayRecordKind::Indeterminate
        ) | (
            ReplayRecordKind::Active,
            ReplayRecordKind::NonIdempotentTerminal | ReplayRecordKind::Indeterminate
        )
    );
    if legal {
        Ok(())
    } else {
        Err(ReplayStoreError::integrity(
            "command replay transition regresses or changes a settled receipt",
        ))
    }
}

const fn phase_tag(kind: ReplayRecordKind) -> i64 {
    match kind {
        ReplayRecordKind::Reserved => 1,
        ReplayRecordKind::Active => 2,
        ReplayRecordKind::NonIdempotentTerminal => 3,
        ReplayRecordKind::ReplayTerminal => 4,
        ReplayRecordKind::Indeterminate => 5,
    }
}

fn receipt_digest(
    action_id: ActionId,
    sequence: i64,
    replay_identity: Sha256Digest,
    reservation_owner: Option<ReplayReservationOwner>,
    phase: i64,
    terminal: Option<&[u8]>,
) -> Sha256Digest {
    let terminal = terminal.unwrap_or_default();
    let mut bytes = Vec::with_capacity(128_usize.saturating_add(terminal.len()));
    if reservation_owner.is_some() {
        bytes.extend_from_slice(b"peritus.command-replay-receipt.v2\0");
    } else {
        bytes.extend_from_slice(b"peritus.command-replay-receipt.v1\0");
    }
    bytes.extend_from_slice(action_id.as_bytes());
    bytes.extend_from_slice(&sequence.to_be_bytes());
    bytes.extend_from_slice(replay_identity.as_bytes());
    if let Some(owner) = reservation_owner {
        bytes.extend_from_slice(owner.as_bytes());
    }
    bytes.extend_from_slice(&phase.to_be_bytes());
    let terminal_length = u64::try_from(terminal.len()).unwrap_or(u64::MAX);
    bytes.extend_from_slice(&terminal_length.to_be_bytes());
    bytes.extend_from_slice(terminal);
    peritus_codec::sha256(&bytes)
}

fn storage(operation: &str, error: rusqlite::Error) -> ReplayStoreError {
    let detail = format!("{operation}: {error}");
    if matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if matches!(
                failure.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            )
    ) {
        ReplayStoreError::contended(detail)
    } else {
        ReplayStoreError::new(detail)
    }
}

struct Contention {
    started: Instant,
    next_report: Duration,
    delay: Duration,
}

impl Contention {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            next_report: CONTENTION_REPORT_INTERVAL,
            delay: RETRY_MINIMUM,
        }
    }

    fn wait(
        &mut self,
        operation: &str,
        cancellation: &CancellationToken,
    ) -> Result<(), OperationError> {
        if cancellation.is_cancelled() {
            return Err(OperationError::Cancelled);
        }
        let elapsed = self.started.elapsed();
        if elapsed >= self.next_report {
            crate::diagnostic::report(&format!(
                "peritus command runtime: {operation} remains queued after {} ms",
                elapsed.as_millis()
            ));
            self.next_report = self
                .next_report
                .checked_add(CONTENTION_REPORT_INTERVAL)
                .unwrap_or(Duration::MAX);
        }
        thread::sleep(self.delay);
        self.delay = cmp::min(self.delay.saturating_mul(2), RETRY_MAXIMUM);
        if cancellation.is_cancelled() {
            Err(OperationError::Cancelled)
        } else {
            Ok(())
        }
    }
}
