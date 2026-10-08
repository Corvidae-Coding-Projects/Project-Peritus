//! Durable allocation of fresh command identities across runtime instances.
//!
//! Effect receipts decide whether a request may execute before shell allocation.
//! This allocator reserves numbers for admitted new work; it does not recover or
//! redispatch previous effects. Failed starts deliberately leave reserved gaps.

use std::{
    cmp,
    collections::VecDeque,
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

use peritus_types::{ActionId, RunId};
use peritus_provider_core::CancellationToken;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::{contract, identity};

const DATABASE_NAME: &str = "command-ordinals.sqlite3";
const RETRY_MINIMUM: Duration = Duration::from_millis(2);
const RETRY_MAXIMUM: Duration = Duration::from_millis(100);
const CONTENTION_REPORT_INTERVAL: Duration = Duration::from_secs(5);
static CLEANUP_SCHEDULER: OnceLock<CleanupScheduler> = OnceLock::new();
// This is a SQLite representation boundary only. The pair is always recombined into the same
// monotonically increasing u64 before contract and CommandIds derivation, so rollover cannot
// create a second identity namespace or alias any pre-migration action.
const ORDINAL_RADIX: u64 = 1_u64 << 63;

/// Failure to settle one durable ordinal reservation.
pub(super) enum ReserveError {
    /// The caller requested cancellation before command dispatch.
    Cancelled,
    /// Durable allocation or integrity validation failed.
    Storage(String),
}

impl From<String> for ReserveError {
    fn from(error: String) -> Self {
        Self::Storage(error)
    }
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS command_ordinals (
    run_id BLOB PRIMARY KEY NOT NULL CHECK(length(run_id) = 16),
    last_ordinal INTEGER NOT NULL
        CHECK(typeof(last_ordinal) = 'integer' AND last_ordinal >= 0)
);
CREATE TABLE IF NOT EXISTS command_ordinal_state_v2 (
    run_id BLOB PRIMARY KEY NOT NULL CHECK(length(run_id) = 16),
    epoch INTEGER NOT NULL CHECK(typeof(epoch) = 'integer' AND epoch BETWEEN 0 AND 1),
    ordinal_offset INTEGER NOT NULL
        CHECK(typeof(ordinal_offset) = 'integer' AND ordinal_offset >= 0)
);
CREATE TABLE IF NOT EXISTS command_ordinal_queue_clock_v2 (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton = 1),
    epoch INTEGER NOT NULL CHECK(typeof(epoch) = 'integer' AND epoch >= 0),
    queue_offset INTEGER NOT NULL
        CHECK(typeof(queue_offset) = 'integer' AND queue_offset >= 0)
);
INSERT INTO command_ordinal_queue_clock_v2(singleton, epoch, queue_offset)
VALUES (1, 0, 0)
ON CONFLICT(singleton) DO NOTHING;
CREATE TABLE IF NOT EXISTS command_ordinal_requests_v2 (
    request_id BLOB PRIMARY KEY NOT NULL CHECK(length(request_id) = 32),
    run_id BLOB NOT NULL CHECK(length(run_id) = 16),
    queue_epoch INTEGER NOT NULL CHECK(typeof(queue_epoch) = 'integer' AND queue_epoch >= 0),
    queue_offset INTEGER NOT NULL
        CHECK(typeof(queue_offset) = 'integer' AND queue_offset >= 0),
    after_epoch INTEGER NOT NULL
        CHECK(typeof(after_epoch) = 'integer' AND after_epoch BETWEEN 0 AND 1),
    after_offset INTEGER NOT NULL
        CHECK(typeof(after_offset) = 'integer' AND after_offset >= 0),
    state INTEGER NOT NULL CHECK(state BETWEEN 0 AND 2),
    reserved_epoch INTEGER,
    reserved_offset INTEGER,
    failure TEXT,
    UNIQUE(queue_epoch, queue_offset),
    CHECK(reserved_epoch IS NULL OR reserved_epoch BETWEEN 0 AND 1),
    CHECK(reserved_offset IS NULL OR reserved_offset >= 0),
    CHECK(
        (state = 0 AND reserved_epoch IS NULL AND reserved_offset IS NULL AND failure IS NULL)
        OR (state = 1 AND reserved_epoch IS NOT NULL AND reserved_offset IS NOT NULL
            AND failure IS NULL)
        OR (state = 2 AND reserved_epoch IS NULL AND reserved_offset IS NULL
            AND failure IS NOT NULL)
    )
);
CREATE INDEX IF NOT EXISTS command_ordinal_pending_v2
ON command_ordinal_requests_v2(state, queue_epoch, queue_offset);
CREATE INDEX IF NOT EXISTS command_ordinal_receipt_frontier_v2
ON command_ordinal_requests_v2(run_id, state, reserved_epoch, reserved_offset);
INSERT INTO command_ordinal_state_v2(run_id, epoch, ordinal_offset)
SELECT run_id, 0, last_ordinal
FROM command_ordinals
WHERE typeof(last_ordinal) = 'integer' AND last_ordinal >= 0
ON CONFLICT(run_id) DO NOTHING;
";

/// Reserves a number before any authority journal or process can be created.
/// `SQLite` serializes reservations across independently opened runtimes. Legacy
/// authority and compactor paths remain occupied even without an allocator row.
pub(super) fn reserve(root: &Path, run_id: RunId, after: u64) -> Result<u64, String> {
    let cancellation = CancellationToken::new();
    reserve_cancellable(root, run_id, after, &cancellation).map_err(|error| match error {
        ReserveError::Cancelled => "command ordinal reservation was cancelled".to_owned(),
        ReserveError::Storage(error) => error,
    })
}

/// Reserves a fresh ordinal while retaining a durable cancellation decision for queued work.
pub(super) fn reserve_cancellable(
    root: &Path,
    run_id: RunId,
    after: u64,
    cancellation: &CancellationToken,
) -> Result<u64, ReserveError> {
    ensure_active(cancellation)?;
    let path = root.join(DATABASE_NAME);
    let mut contention = Contention::new();
    let mut connection = loop {
        ensure_active(cancellation)?;
        match Connection::open(&path) {
            Ok(connection) => break connection,
            Err(error) if is_recoverable_wait(&error) => {
                contention.wait("open durable ordinal store", cancellation)?;
            }
            Err(error) => return Err(ReserveError::Storage(detail(&error))),
        }
    };
    // SQLite's busy handler is a patience deadline. Disable it and retry through
    // the durable queue so lock ownership can outlive any arbitrary elapsed time.
    retry_sqlite(
        &mut contention,
        cancellation,
        "configure durable ordinal contention",
        || connection.busy_timeout(Duration::ZERO),
    )?;
    // EXTRA also syncs the rollback-journal directory after commit. FULL alone
    // can lose the last acknowledged reservation on power loss in DELETE mode.
    retry_sqlite(
        &mut contention,
        cancellation,
        "configure durable ordinal store",
        || connection.pragma_update(None, "synchronous", "EXTRA"),
    )?;
    initialize(&mut connection, &mut contention, cancellation)?;

    let request_id = request_id().map_err(ReserveError::Storage)?;
    // Establish a process-owned cleanup destination before the request can become durable. Once
    // armed, every early return transfers this exact request identity without waiting for SQLite.
    let cleanup = cleanup_scheduler().map_err(ReserveError::Storage)?;
    let mut owner =
        RequestOwner::new(cleanup, CleanupTask::new(path, run_id, after, request_id));
    enqueue(
        &mut connection,
        run_id,
        after,
        &request_id,
        &mut contention,
        cancellation,
        &mut owner,
    )?;
    loop {
        if cancellation.is_cancelled() {
            return Err(ReserveError::Cancelled);
        }
        match request_result(&connection, run_id, after, &request_id) {
            Ok(RequestResult::Allocated(ordinal)) => {
                owner.settled();
                return Ok(ordinal);
            }
            Ok(RequestResult::Rejected(failure)) => {
                owner.settled();
                return Err(ReserveError::Storage(failure));
            }
            Ok(RequestResult::Pending) => {}
            Err(error) if is_recoverable_wait(&error) => {
                contention.wait("read durable ordinal request", cancellation)?;
                continue;
            }
            Err(error) => return Err(ReserveError::Storage(detail(&error))),
        }
        match drive_queue(root, &mut connection, &mut contention, cancellation) {
            Ok(()) => {}
            Err(ReserveError::Cancelled) => return Err(ReserveError::Cancelled),
            Err(error) => return Err(error),
        }
    }
}

fn initialize(
    connection: &mut Connection,
    contention: &mut Contention,
    cancellation: &CancellationToken,
) -> Result<(), ReserveError> {
    loop {
        ensure_active(cancellation)?;
        let transaction = match connection.transaction_with_behavior(TransactionBehavior::Immediate)
        {
            Ok(transaction) => transaction,
            Err(error) if is_recoverable_wait(&error) => {
                contention.wait("initialize durable ordinal store", cancellation)?;
                continue;
            }
            Err(error) => return Err(ReserveError::Storage(detail(&error))),
        };
        if let Err(error) = transaction.execute_batch(SCHEMA) {
            drop(transaction);
            if is_recoverable_wait(&error) {
                contention.wait("initialize durable ordinal store", cancellation)?;
                continue;
            }
            return Err(ReserveError::Storage(detail(&error)));
        }
        match transaction.commit() {
            Ok(()) => return Ok(()),
            Err(error) if is_recoverable_wait(&error) => {
                contention.wait("publish durable ordinal schema", cancellation)?;
            }
            Err(error) => return Err(ReserveError::Storage(detail(&error))),
        }
    }
}

fn enqueue(
    connection: &mut Connection,
    run_id: RunId,
    after: u64,
    request_id: &[u8; 32],
    contention: &mut Contention,
    cancellation: &CancellationToken,
    owner: &mut RequestOwner,
) -> Result<(), ReserveError> {
    let (after_epoch, after_offset) = split_ordinal(after);
    let mut publication_ambiguous = false;
    loop {
        match request_result(connection, run_id, after, request_id) {
            Ok(RequestResult::Pending | RequestResult::Allocated(_) | RequestResult::Rejected(_)) => {
                owner.arm();
                return Ok(());
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                publication_ambiguous = false;
                ensure_active(cancellation)?;
            }
            Err(error) if is_recoverable_wait(&error) => {
                let operation = if publication_ambiguous {
                    "reconcile durable ordinal request"
                } else {
                    "read durable ordinal request"
                };
                contention.wait(operation, cancellation)?;
                continue;
            }
            Err(error) => return Err(ReserveError::Storage(detail(&error))),
        }
        let transaction = match connection.transaction_with_behavior(TransactionBehavior::Immediate)
        {
            Ok(transaction) => transaction,
            Err(error) if is_recoverable_wait(&error) => {
                contention.wait("enqueue durable ordinal request", cancellation)?;
                continue;
            }
            Err(error) => return Err(ReserveError::Storage(detail(&error))),
        };
        let attempt = (|| {
            transaction.execute(
                "INSERT INTO command_ordinals(run_id, last_ordinal) VALUES (?1, 0)
                 ON CONFLICT(run_id) DO NOTHING",
                params![run_id.as_bytes().as_slice()],
            )?;
            transaction.execute(
                "INSERT INTO command_ordinal_state_v2(run_id, epoch, ordinal_offset)
                 SELECT run_id, 0, last_ordinal FROM command_ordinals WHERE run_id = ?1
                 ON CONFLICT(run_id) DO NOTHING",
                params![run_id.as_bytes().as_slice()],
            )?;
            let (queue_epoch, queue_offset): (i64, i64) = transaction.query_row(
                "SELECT epoch, queue_offset FROM command_ordinal_queue_clock_v2
                 WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let (queue_epoch, queue_offset) = next_pair(queue_epoch, queue_offset)
                .map_err(sql_conversion)?;
            transaction.execute(
                "UPDATE command_ordinal_queue_clock_v2 SET epoch = ?1, queue_offset = ?2
                 WHERE singleton = 1",
                params![queue_epoch, queue_offset],
            )?;
            transaction.execute(
                "INSERT INTO command_ordinal_requests_v2(
                    request_id, run_id, queue_epoch, queue_offset,
                    after_epoch, after_offset, state
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
                params![
                    request_id.as_slice(),
                    run_id.as_bytes().as_slice(),
                    queue_epoch,
                    queue_offset,
                    after_epoch,
                    after_offset,
                ],
            )?;
            Ok::<(), rusqlite::Error>(())
        })();
        if let Err(error) = attempt {
            drop(transaction);
            if is_recoverable_wait(&error) {
                contention.wait("enqueue durable ordinal request", cancellation)?;
                continue;
            }
            return Err(ReserveError::Storage(detail(&error)));
        }
        publication_ambiguous = true;
        owner.arm();
        match transaction.commit() {
            Ok(()) => return Ok(()),
            Err(error) if is_recoverable_wait(&error) => {
                contention.wait("publish durable ordinal request", cancellation)?;
            }
            Err(error) => {
                // COMMIT errors can be ambiguous. Keep the same request identity and
                // reconcile it before any later enqueue attempt.
                match request_result(connection, run_id, after, request_id) {
                    Ok(_) => return Ok(()),
                    Err(reconcile) if is_recoverable_wait(&reconcile) => {
                        contention.wait("reconcile durable ordinal request", cancellation)?;
                    }
                    Err(rusqlite::Error::QueryReturnedNoRows) => {
                        return Err(ReserveError::Storage(detail(&error)));
                    }
                    Err(reconcile) => {
                        return Err(ReserveError::Storage(format!(
                            "{}; reconcile durable command ordinal request: {reconcile}",
                            detail(&error)
                        )));
                    }
                }
            }
        }
    }
}

fn drive_queue(
    root: &Path,
    connection: &mut Connection,
    contention: &mut Contention,
    cancellation: &CancellationToken,
) -> Result<(), ReserveError> {
    loop {
        ensure_active(cancellation)?;
        let transaction = match connection.transaction_with_behavior(TransactionBehavior::Immediate)
        {
            Ok(transaction) => transaction,
            Err(error) if is_recoverable_wait(&error) => {
                contention.wait("acquire durable ordinal owner queue", cancellation)?;
                continue;
            }
            Err(error) => return Err(ReserveError::Storage(detail(&error))),
        };
        let raw = transaction
            .query_row(
                "SELECT request_id, run_id, after_epoch, after_offset
                 FROM command_ordinal_requests_v2
                 WHERE state = 0
                 ORDER BY queue_epoch, queue_offset
                 LIMIT 1",
                [],
                |row| {
                    Ok(PendingRequest {
                        request_id: row.get(0)?,
                        run_id: row.get(1)?,
                        after_epoch: row.get(2)?,
                        after_offset: row.get(3)?,
                    })
                },
            )
            .optional();
        let pending = match raw {
            Ok(Some(pending)) => pending,
            Ok(None) => return Ok(()),
            Err(error) => {
                drop(transaction);
                if is_recoverable_wait(&error) {
                    contention.wait("read durable ordinal owner queue", cancellation)?;
                    continue;
                }
                return Err(ReserveError::Storage(detail(&error)));
            }
        };
        let allocation = allocate_pending(root, &transaction, &pending);
        let update = match allocation {
            Ok(ordinal) => {
                let (epoch, offset) = split_ordinal(ordinal);
                transaction
                    .execute(
                        "INSERT INTO command_ordinal_state_v2(run_id, epoch, ordinal_offset)
                         VALUES (?1, ?2, ?3)
                         ON CONFLICT(run_id) DO UPDATE
                         SET epoch = excluded.epoch, ordinal_offset = excluded.ordinal_offset",
                        params![pending.run_id.as_slice(), epoch, offset],
                    )
                    .and_then(|_| {
                        transaction.execute(
                            "UPDATE command_ordinals SET last_ordinal = ?1 WHERE run_id = ?2",
                            params![legacy_ordinal(ordinal), pending.run_id.as_slice()],
                        )
                    })
                    .and_then(|_| {
                        transaction.execute(
                            "UPDATE command_ordinal_requests_v2
                             SET state = 1, reserved_epoch = ?1, reserved_offset = ?2
                             WHERE request_id = ?3 AND state = 0",
                            params![epoch, offset, pending.request_id.as_slice()],
                        )
                    })
            }
            Err(failure) => transaction.execute(
                "UPDATE command_ordinal_requests_v2
                 SET state = 2, failure = ?1
                 WHERE request_id = ?2 AND state = 0",
                params![failure, pending.request_id.as_slice()],
            ),
        };
        match update {
            Ok(1) => {}
            Ok(_) => {
                return Err(ReserveError::Storage(
                    "durable command ordinal request changed ownership while locked".to_owned(),
                ));
            }
            Err(error) => {
                drop(transaction);
                if is_recoverable_wait(&error) {
                    contention.wait("advance durable ordinal owner queue", cancellation)?;
                    continue;
                }
                return Err(ReserveError::Storage(detail(&error)));
            }
        }
        match transaction.commit() {
            Ok(()) => return Ok(()),
            Err(error) if is_recoverable_wait(&error) => {
                contention.wait("publish durable ordinal allocation", cancellation)?;
            }
            Err(error) => match request_state(connection, &pending.request_id) {
                Ok(state) if state != 0 => return Ok(()),
                Ok(_) => return Err(ReserveError::Storage(detail(&error))),
                Err(reconcile) if is_recoverable_wait(&reconcile) => {
                    contention.wait("reconcile durable ordinal allocation", cancellation)?;
                }
                Err(reconcile) => {
                    return Err(ReserveError::Storage(format!(
                        "{}; reconcile durable command ordinal allocation: {reconcile}",
                        detail(&error)
                    )));
                }
            },
        }
    }
}

struct CleanupTask {
    path: PathBuf,
    run_id: RunId,
    after: u64,
    request_id: [u8; 32],
    ready_at: Instant,
    next_report: Instant,
    delay: Duration,
    attempts: u64,
}

impl CleanupTask {
    fn new(path: PathBuf, run_id: RunId, after: u64, request_id: [u8; 32]) -> Self {
        let now = Instant::now();
        Self {
            path,
            run_id,
            after,
            request_id,
            ready_at: now,
            next_report: now.checked_add(CONTENTION_REPORT_INTERVAL).unwrap_or(now),
            delay: RETRY_MINIMUM,
            attempts: 0,
        }
    }

    fn defer(&mut self, operation: &'static str, failure: Option<&str>) {
        self.attempts = self.attempts.saturating_add(1);
        let now = Instant::now();
        if (failure.is_some() && self.attempts == 1) || now >= self.next_report {
            match failure {
                Some(failure) => crate::diagnostic::report(&format!(
                    "peritus command runtime: {operation} remains owned after {} cleanup attempts: {failure}",
                    self.attempts
                )),
                None => crate::diagnostic::report(&format!(
                    "peritus command runtime: {operation} remains owned after {} cleanup attempts",
                    self.attempts
                )),
            }
            self.next_report = now.checked_add(CONTENTION_REPORT_INTERVAL).unwrap_or(now);
        }
        self.ready_at = now.checked_add(self.delay).unwrap_or(now);
        self.delay = cmp::min(self.delay.saturating_mul(2), RETRY_MAXIMUM);
    }
}

#[derive(Clone)]
struct CleanupScheduler {
    state: Arc<CleanupSchedulerState>,
}

struct CleanupSchedulerState {
    pending: Mutex<VecDeque<CleanupTask>>,
    wake: Condvar,
    dispatcher_started: Mutex<bool>,
}

impl CleanupScheduler {
    fn new() -> Self {
        Self {
            state: Arc::new(CleanupSchedulerState {
                pending: Mutex::new(VecDeque::new()),
                wake: Condvar::new(),
                dispatcher_started: Mutex::new(false),
            }),
        }
    }

    fn ensure_running(&self) -> Result<(), String> {
        let mut started = self
            .state
            .dispatcher_started
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *started {
            return Ok(());
        }
        let state = Arc::clone(&self.state);
        let dispatcher = thread::Builder::new()
            .name("peritus-command-ordinal-cleanup".to_owned())
            .spawn(move || run_cleanup_dispatcher(state))
            .map_err(|error| format!("start durable command ordinal cleanup worker: {error}"))?;
        drop(dispatcher);
        *started = true;
        Ok(())
    }

    fn enqueue(&self, task: CleanupTask) {
        self.state
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_back(task);
        self.state.wake.notify_one();
    }
}

impl CleanupSchedulerState {
    fn take(&self) -> CleanupTask {
        let mut pending =
            self.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if let Some(task) = pending.pop_front() {
                return task;
            }
            pending = self
                .wake
                .wait(pending)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    fn enqueue(&self, task: CleanupTask) {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_back(task);
        self.wake.notify_one();
    }
}

fn cleanup_scheduler() -> Result<CleanupScheduler, String> {
    let scheduler = CLEANUP_SCHEDULER.get_or_init(CleanupScheduler::new).clone();
    scheduler.ensure_running()?;
    Ok(scheduler)
}

struct RequestOwner {
    scheduler: CleanupScheduler,
    task: Option<CleanupTask>,
    armed: bool,
}

impl RequestOwner {
    fn new(scheduler: CleanupScheduler, task: CleanupTask) -> Self {
        Self { scheduler, task: Some(task), armed: false }
    }

    fn arm(&mut self) {
        self.armed = true;
    }

    fn settled(&mut self) {
        self.armed = false;
        self.task = None;
    }
}

impl Drop for RequestOwner {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let Some(task) = self.task.take() else { return };
        self.scheduler.enqueue(task);
    }
}

struct CleanupLease {
    state: Arc<CleanupSchedulerState>,
    task: Option<CleanupTask>,
}

impl CleanupLease {
    fn new(state: Arc<CleanupSchedulerState>, task: CleanupTask) -> Self {
        Self { state, task: Some(task) }
    }

    fn task(&self) -> &CleanupTask {
        self.task.as_ref().expect("cleanup lease retains exact work")
    }

    fn task_mut(&mut self) -> &mut CleanupTask {
        self.task.as_mut().expect("cleanup lease retains exact work")
    }

    fn take(&mut self) -> CleanupTask {
        self.task.take().expect("cleanup lease retains exact work")
    }

    fn settled(&mut self) {
        self.task = None;
    }
}

impl Drop for CleanupLease {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            self.state.enqueue(task);
        }
    }
}

struct CleanupSlot {
    state: Arc<CleanupSchedulerState>,
    task: Mutex<Option<CleanupTask>>,
}

impl Drop for CleanupSlot {
    fn drop(&mut self) {
        if let Some(task) = self
            .task
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            self.state.enqueue(task);
        }
    }
}

fn run_cleanup_dispatcher(state: Arc<CleanupSchedulerState>) {
    loop {
        if catch_unwind(AssertUnwindSafe(|| dispatch_cleanup_once(&state))).is_err() {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                crate::diagnostic::report(
                    "peritus command runtime: durable ordinal cleanup dispatcher panicked",
                );
            }));
        }
    }
}

fn dispatch_cleanup_once(state: &Arc<CleanupSchedulerState>) {
    let task = state.take();
    let mut lease = CleanupLease::new(Arc::clone(state), task);
    let slot = Arc::new(CleanupSlot {
        state: Arc::clone(state),
        task: Mutex::new(Some(lease.take())),
    });
    let worker_slot = Arc::clone(&slot);
    match thread::Builder::new()
        .name("peritus-command-ordinal-cleanup-job".to_owned())
        .spawn(move || run_cleanup_job(worker_slot))
    {
        Ok(worker) => drop(worker),
        Err(error) => {
            let mut retained = slot.task.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(task) = retained.as_mut() {
                task.defer("start durable ordinal cleanup job", Some(&error.to_string()));
            }
            drop(retained);
            thread::sleep(RETRY_MINIMUM);
        }
    }
}

fn run_cleanup_job(slot: Arc<CleanupSlot>) {
    let task = slot
        .task
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    let Some(task) = task else { return };
    let mut lease = CleanupLease::new(Arc::clone(&slot.state), task);
    loop {
        let wait = lease.task().ready_at.saturating_duration_since(Instant::now());
        if !wait.is_zero() {
            thread::sleep(wait);
        }
        match catch_unwind(AssertUnwindSafe(|| settle_cleanup_step(lease.task()))) {
            Ok(CleanupStep::Settled) => {
                lease.settled();
                return;
            }
            Ok(CleanupStep::Retry(operation)) => lease.task_mut().defer(operation, None),
            Ok(CleanupStep::Failed { operation, detail }) => {
                lease.task_mut().defer(operation, Some(&detail));
            }
            Err(_) => lease
                .task_mut()
                .defer("settle cancelled durable ordinal request", Some("worker panicked")),
        }
    }
}

enum CleanupStep {
    Settled,
    Retry(&'static str),
    Failed { operation: &'static str, detail: String },
}

fn settle_cleanup_step(task: &CleanupTask) -> CleanupStep {
    let mut connection = match Connection::open(&task.path) {
        Ok(connection) => connection,
        Err(error) if is_recoverable_wait(&error) => {
            return CleanupStep::Retry("reopen cancelled durable ordinal request");
        }
        Err(error) => {
            return cleanup_failure("reopen cancelled durable ordinal request", &error);
        }
    };
    match connection.busy_timeout(Duration::ZERO) {
        Ok(()) => {}
        Err(error) if is_recoverable_wait(&error) => {
            return CleanupStep::Retry("configure cancelled durable ordinal request");
        }
        Err(error) => {
            return cleanup_failure("configure cancelled durable ordinal request", &error);
        }
    }
    match connection.pragma_update(None, "synchronous", "EXTRA") {
        Ok(()) => {}
        Err(error) if is_recoverable_wait(&error) => {
            return CleanupStep::Retry("configure cancelled durable ordinal request");
        }
        Err(error) => {
            return cleanup_failure("configure cancelled durable ordinal request", &error);
        }
    }
    match request_result(&connection, task.run_id, task.after, &task.request_id) {
        Ok(RequestResult::Pending) => {
            cancel_request_step(&mut connection, &task.request_id)
        }
        Ok(RequestResult::Allocated(_) | RequestResult::Rejected(_))
        | Err(rusqlite::Error::QueryReturnedNoRows) => CleanupStep::Settled,
        Err(error) if is_recoverable_wait(&error) => {
            CleanupStep::Retry("reconcile cancelled durable ordinal request")
        }
        Err(error) => cleanup_failure("reconcile cancelled durable ordinal request", &error),
    }
}

fn cancel_request_step(
    connection: &mut Connection,
    request_id: &[u8; 32],
) -> CleanupStep {
    const CANCELLED: &str = "command ordinal reservation was cancelled before dispatch";
    let transaction = match connection.transaction_with_behavior(TransactionBehavior::Immediate) {
        Ok(transaction) => transaction,
        Err(error) if is_recoverable_wait(&error) => {
            return CleanupStep::Retry("settle cancelled durable ordinal request");
        }
        Err(error) => return cleanup_failure("settle cancelled durable ordinal request", &error),
    };
    let state: i64 = match transaction.query_row(
        "SELECT state FROM command_ordinal_requests_v2 WHERE request_id = ?1",
        params![request_id.as_slice()],
        |row| row.get(0),
    ) {
        Ok(state) => state,
        Err(error) if is_recoverable_wait(&error) => {
            return CleanupStep::Retry("read cancelled durable ordinal request");
        }
        Err(error) => return cleanup_failure("read cancelled durable ordinal request", &error),
    };
    match state {
        0 => {}
        1 | 2 => return CleanupStep::Settled,
        _ => {
            return CleanupStep::Failed {
                operation: "read cancelled durable ordinal request",
                detail: "cancelled command ordinal request has an invalid durable state".to_owned(),
            };
        }
    }
    match transaction.execute(
        "UPDATE command_ordinal_requests_v2
         SET state = 2, failure = ?1
         WHERE request_id = ?2 AND state = 0",
        params![CANCELLED, request_id.as_slice()],
    ) {
        Ok(1) => {}
        Ok(_) => {
            return CleanupStep::Failed {
                operation: "settle cancelled durable ordinal request",
                detail: "cancelled command ordinal request changed ownership while locked"
                    .to_owned(),
            };
        }
        Err(error) if is_recoverable_wait(&error) => {
            return CleanupStep::Retry("settle cancelled durable ordinal request");
        }
        Err(error) => return cleanup_failure("settle cancelled durable ordinal request", &error),
    }
    match transaction.commit() {
        Ok(()) => CleanupStep::Settled,
        Err(error) if is_recoverable_wait(&error) => {
            CleanupStep::Retry("publish cancelled durable ordinal request")
        }
        Err(error) => match request_state(connection, request_id) {
            Ok(1 | 2) => CleanupStep::Settled,
            Ok(0) => cleanup_failure("publish cancelled durable ordinal request", &error),
            Ok(_) => CleanupStep::Failed {
                operation: "reconcile cancelled durable ordinal request",
                detail: "cancelled command ordinal request has an invalid durable state".to_owned(),
            },
            Err(reconcile) if is_recoverable_wait(&reconcile) => {
                CleanupStep::Retry("reconcile cancelled durable ordinal request")
            }
            Err(reconcile) => CleanupStep::Failed {
                operation: "reconcile cancelled durable ordinal request",
                detail: format!("{}; reconcile command ordinal request: {reconcile}", detail(&error)),
            },
        },
    }
}

fn cleanup_failure(operation: &'static str, error: &rusqlite::Error) -> CleanupStep {
    CleanupStep::Failed { operation, detail: detail(error) }
}

fn allocate_pending(
    root: &Path,
    transaction: &rusqlite::Transaction<'_>,
    pending: &PendingRequest,
) -> Result<u64, String> {
    let run_bytes: [u8; 16] = pending
        .run_id
        .as_slice()
        .try_into()
        .map_err(|_| "durable command ordinal request contains a malformed run identity".to_owned())?;
    let run_id = RunId::new(run_bytes)
        .map_err(|error| format!("reconstruct durable command ordinal run identity: {error:?}"))?;
    let after = join_ordinal(pending.after_epoch, pending.after_offset)?;
    let legacy: i64 = transaction
        .query_row(
            "SELECT last_ordinal FROM command_ordinals WHERE run_id = ?1",
            params![pending.run_id.as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| detail(&error))?;
    let legacy = u64::try_from(legacy)
        .map_err(|_| "command ordinal store contains a negative number".to_owned())?;
    let state: Option<(i64, i64)> = transaction
        .query_row(
            "SELECT epoch, ordinal_offset FROM command_ordinal_state_v2 WHERE run_id = ?1",
            params![pending.run_id.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| detail(&error))?;
    // Allocation receipts are authoritative too. Taking their frontier prevents a damaged or
    // stale derived high-water row from making an already accepted identity reusable.
    let receipt: Option<(i64, i64)> = transaction
        .query_row(
            "SELECT reserved_epoch, reserved_offset
             FROM command_ordinal_requests_v2
             WHERE run_id = ?1 AND state = 1
             ORDER BY reserved_epoch DESC, reserved_offset DESC
             LIMIT 1",
            params![pending.run_id.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| detail(&error))?;
    let durable = state
        .map(|(epoch, offset)| join_ordinal(epoch, offset))
        .transpose()?
        .unwrap_or(0)
        .max(
            receipt
                .map(|(epoch, offset)| join_ordinal(epoch, offset))
                .transpose()?
                .unwrap_or(0),
        )
        .max(legacy);
    let mut ordinal = durable.max(after);
    loop {
        ordinal = ordinal
            .checked_add(1)
            .ok_or_else(|| "command runtime action ordinal overflowed".to_owned())?;
        let action = ActionId::new(contract::id(run_id, ordinal, "action"))
            .map_err(|error| format!("construct reserved command identity: {error:?}"))?;
        let action = identity::action_hex(action);
        if !occupied(&root.join("authority").join(&action))?
            && !occupied(&root.join("local-compactor").join(&action))?
        {
            return Ok(ordinal);
        }
    }
}

fn request_result(
    connection: &Connection,
    run_id: RunId,
    after: u64,
    request_id: &[u8; 32],
) -> Result<RequestResult, rusqlite::Error> {
    let row: (Vec<u8>, i64, i64, i64, Option<i64>, Option<i64>, Option<String>) = connection
        .query_row(
            "SELECT run_id, after_epoch, after_offset, state,
                    reserved_epoch, reserved_offset, failure
             FROM command_ordinal_requests_v2 WHERE request_id = ?1",
            params![request_id.as_slice()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )?;
    let expected = split_ordinal(after);
    if row.0.as_slice() != run_id.as_bytes()
        || (row.1, row.2) != expected
    {
        return Err(rusqlite::Error::InvalidQuery);
    }
    match (row.3, row.4, row.5, row.6) {
        (0, None, None, None) => Ok(RequestResult::Pending),
        (1, Some(epoch), Some(offset), None) => join_ordinal(epoch, offset)
            .map(RequestResult::Allocated)
            .map_err(|message| rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Integer,
                conversion_failure(message),
            )),
        (2, None, None, Some(failure)) => Ok(RequestResult::Rejected(failure)),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

fn request_state(connection: &Connection, request_id: &[u8]) -> Result<i64, rusqlite::Error> {
    connection.query_row(
        "SELECT state FROM command_ordinal_requests_v2 WHERE request_id = ?1",
        params![request_id],
        |row| row.get(0),
    )
}

fn request_id() -> Result<[u8; 32], String> {
    let mut request = [0_u8; 32];
    getrandom::fill(&mut request)
        .map_err(|error| format!("create durable command ordinal request identity: {error}"))?;
    Ok(request)
}

const fn split_ordinal(ordinal: u64) -> (i64, i64) {
    let epoch = (ordinal / ORDINAL_RADIX) as i64;
    let offset = (ordinal % ORDINAL_RADIX) as i64;
    (epoch, offset)
}

fn join_ordinal(epoch: i64, offset: i64) -> Result<u64, String> {
    let epoch = u64::try_from(epoch)
        .map_err(|_| "command ordinal epoch is negative".to_owned())?;
    let offset = u64::try_from(offset)
        .map_err(|_| "command ordinal offset is negative".to_owned())?;
    if epoch > 1 || offset >= ORDINAL_RADIX {
        return Err("command ordinal epoch and offset exceed the action identity width".to_owned());
    }
    epoch
        .checked_mul(ORDINAL_RADIX)
        .and_then(|base| base.checked_add(offset))
        .ok_or_else(|| "command runtime action ordinal overflowed".to_owned())
}

fn next_pair(epoch: i64, offset: i64) -> Result<(i64, i64), String> {
    if epoch < 0 || offset < 0 {
        return Err("durable command ordinal queue clock is negative".to_owned());
    }
    if offset < i64::MAX {
        return Ok((epoch, offset + 1));
    }
    epoch
        .checked_add(1)
        .map(|epoch| (epoch, 0))
        .ok_or_else(|| "durable command ordinal queue clock overflowed".to_owned())
}

fn sql_conversion(message: String) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(conversion_failure(message))
}

fn conversion_failure(message: String) -> Box<dyn std::error::Error + Send + Sync + 'static> {
    Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, message))
}

fn legacy_ordinal(ordinal: u64) -> i64 {
    i64::try_from(ordinal).unwrap_or(i64::MAX)
}

fn retry_sqlite<T>(
    contention: &mut Contention,
    cancellation: &CancellationToken,
    operation: &str,
    mut attempt: impl FnMut() -> Result<T, rusqlite::Error>,
) -> Result<T, ReserveError> {
    loop {
        ensure_active(cancellation)?;
        match attempt() {
            Ok(value) => return Ok(value),
            Err(error) if is_recoverable_wait(&error) => {
                contention.wait(operation, cancellation)?;
            }
            Err(error) => return Err(ReserveError::Storage(detail(&error))),
        }
    }
}

fn ensure_active(cancellation: &CancellationToken) -> Result<(), ReserveError> {
    if cancellation.is_cancelled() {
        Err(ReserveError::Cancelled)
    } else {
        Ok(())
    }
}

fn is_recoverable_wait(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if matches!(
                failure.code,
                rusqlite::ErrorCode::DatabaseBusy
                    | rusqlite::ErrorCode::DatabaseLocked
                    | rusqlite::ErrorCode::DiskFull
                    | rusqlite::ErrorCode::SystemIoFailure
                    | rusqlite::ErrorCode::CannotOpen
                    | rusqlite::ErrorCode::FileLockingProtocolFailed
            )
    )
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
    ) -> Result<(), ReserveError> {
        ensure_active(cancellation)?;
        self.wait_unchecked(operation);
        ensure_active(cancellation)
    }

    fn wait_unchecked(&mut self, operation: &str) {
        let elapsed = self.started.elapsed();
        if elapsed >= self.next_report {
            crate::diagnostic::report(&format!(
                "peritus command runtime: {operation} remains recoverably queued after {} ms",
                elapsed.as_millis()
            ));
            self.next_report = self
                .next_report
                .checked_add(CONTENTION_REPORT_INTERVAL)
                .unwrap_or(Duration::MAX);
        }
        thread::sleep(self.delay);
        self.delay = cmp::min(self.delay.saturating_mul(2), RETRY_MAXIMUM);
    }
}

struct PendingRequest {
    request_id: Vec<u8>,
    run_id: Vec<u8>,
    after_epoch: i64,
    after_offset: i64,
}

enum RequestResult {
    Pending,
    Allocated(u64),
    Rejected(String),
}

fn occupied(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("inspect existing command identity: {error}")),
    }
}

fn detail(error: &rusqlite::Error) -> String {
    format!("reserve durable command ordinal: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopening_and_stale_local_counters_do_not_reuse_reservations() {
        let root = tempfile::tempdir().expect("state directory");
        let run = RunId::new([1; 16]).expect("run");
        assert_eq!(reserve(root.path(), run, 0).expect("first"), 1);
        // Every reserve opens a separate connection. No authority file is needed
        // for a reservation to survive a failed start or runtime restart.
        assert_eq!(reserve(root.path(), run, 0).expect("reopened"), 2);
        assert_eq!(reserve(root.path(), run, 1).expect("stale instance"), 3);
        assert_eq!(reserve(root.path(), run, 9).expect("local high water"), 10);
        assert_eq!(reserve(root.path(), run, 0).expect("reopened again"), 11);
    }

    #[test]
    fn legacy_shell_and_compactor_identities_are_preserved_including_gaps() {
        let root = tempfile::tempdir().expect("state directory");
        let run = RunId::new([2; 16]).expect("run");
        for (ordinal, directory) in [(1, "authority"), (3, "local-compactor")] {
            let action = ActionId::new(contract::id(run, ordinal, "action")).expect("action");
            let path = root.path().join(directory).join(identity::action_hex(action));
            fs::create_dir_all(&path).expect("legacy identity");
            fs::write(path.join("preserved"), b"legacy evidence").expect("evidence");
        }
        assert_eq!(reserve(root.path(), run, 0).expect("first free"), 2);
        assert_eq!(reserve(root.path(), run, 0).expect("skip later legacy"), 4);
        for (ordinal, directory) in [(1, "authority"), (3, "local-compactor")] {
            let action = ActionId::new(contract::id(run, ordinal, "action")).expect("action");
            let path = root.path().join(directory).join(identity::action_hex(action));
            assert_eq!(fs::read(path.join("preserved")).expect("retained"), b"legacy evidence");
        }
    }

    #[test]
    fn independent_threads_reserve_distinct_numbers() {
        for initialized in [false, true] {
            concurrent_reservations(initialized);
        }
    }

    #[test]
    fn transient_writer_contention_waits_for_a_durable_reservation() {
        for initialized in [false, true] {
            let root = tempfile::tempdir().expect("state directory");
            let run = RunId::new([8; 16]).expect("run");
            let offset =
                if initialized { reserve(root.path(), run, 0).expect("initialize") } else { 0 };
            let mut blocker = Connection::open(root.path().join("command-ordinals.sqlite3"))
                .expect("open contending writer");
            let transaction = blocker
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .expect("hold writer lock");
            let barrier = std::sync::Barrier::new(2);
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            std::thread::scope(|scope| {
                let worker = scope.spawn(|| {
                    barrier.wait();
                    sender.send(reserve(root.path(), run, 0)).expect("send reservation");
                });
                barrier.wait();
                // A competing durable writer may outlive the old 250 ms budget.
                // Keep its lock until the waiting reservation has had twice that long.
                let while_locked = receiver.recv_timeout(Duration::from_millis(500));
                transaction.commit().expect("release writer lock");
                worker.join().expect("allocation thread");
                assert!(
                    matches!(while_locked, Err(std::sync::mpsc::RecvTimeoutError::Timeout)),
                    "reservation completed while writer lock held: {while_locked:?}"
                );
                assert_eq!(
                    receiver.recv().expect("reservation result").expect("reserved"),
                    offset + 1
                );
            });
            assert_eq!(reserve(root.path(), run, 0).expect("reopen after contention"), offset + 2);
        }
    }

    fn concurrent_reservations(initialized: bool) {
        let root = tempfile::tempdir().expect("state directory");
        let run = RunId::new([3; 16]).expect("run");
        let offset =
            if initialized { reserve(root.path(), run, 0).expect("initialize") } else { 0 };
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
        // Start every worker before joining: joining a lazy spawn iterator would deadlock
        // the barrier and would no longer exercise simultaneous reservations.
        let mut handles = Vec::with_capacity(4);
        for _ in 0..4 {
            let path = root.path().to_path_buf();
            let barrier = std::sync::Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                reserve(&path, run, 0).expect("concurrent allocation")
            }));
        }
        let mut values: Vec<_> =
            handles.into_iter().map(|handle| handle.join().expect("allocation thread")).collect();
        values.sort_unstable();
        assert_eq!(values, vec![offset + 1, offset + 2, offset + 3, offset + 4]);
        assert_eq!(reserve(root.path(), run, 0).expect("reopen after contention"), offset + 5);
    }

    #[test]
    fn separate_runs_retain_independent_high_water_marks() {
        let root = tempfile::tempdir().expect("state directory");
        let first = RunId::new([5; 16]).expect("first run");
        let second = RunId::new([6; 16]).expect("second run");
        assert_eq!(reserve(root.path(), first, 0).expect("first reservation"), 1);
        assert_eq!(reserve(root.path(), second, 0).expect("second run reservation"), 1);
        assert_eq!(reserve(root.path(), first, 0).expect("first run reopened"), 2);
        assert_eq!(reserve(root.path(), second, 0).expect("second run reopened"), 2);
    }

    #[test]
    fn invalid_and_exhausted_stored_counters_are_never_reset() {
        for stored in [
            rusqlite::types::Value::Integer(-1),
            rusqlite::types::Value::Text("invalid ordinal".to_owned()),
            rusqlite::types::Value::Integer(i64::MAX),
        ] {
            let root = tempfile::tempdir().expect("state directory");
            let run = RunId::new([7; 16]).expect("run");
            assert_eq!(reserve(root.path(), run, 0).expect("initialize"), 1);
            let connection = Connection::open(root.path().join("command-ordinals.sqlite3"))
                .expect("open fixture");
            connection
                .pragma_update(None, "ignore_check_constraints", true)
                .expect("permit corrupt fixture");
            connection
                .execute("UPDATE command_ordinals SET last_ordinal = ?1", params![stored])
                .expect("write stored counter fixture");
            assert!(reserve(root.path(), run, 0).is_err());
            let retained: rusqlite::types::Value = connection
                .query_row("SELECT last_ordinal FROM command_ordinals", [], |row| row.get(0))
                .expect("read retained counter");
            assert_eq!(retained, stored);
        }
    }

    #[test]
    fn malformed_store_and_exhaustion_fail_without_resetting_identity() {
        let root = tempfile::tempdir().expect("state directory");
        let run = RunId::new([4; 16]).expect("run");
        assert!(reserve(root.path(), run, u64::MAX).is_err());
        fs::write(root.path().join("command-ordinals.sqlite3"), b"invalid database")
            .expect("damaged fixture");
        assert!(reserve(root.path(), run, 0).is_err());
    }
}
