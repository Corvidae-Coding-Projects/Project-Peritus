//! Runtime-owned observer reconciliation for accepted commands.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use peritus_process::ProcessControl;

use super::CommandRuntime;

mod lane;
use lane::{TaskLane, TaskOwners};

const RETRY_INTERVAL: Duration = Duration::from_millis(20);
const REPORT_INTERVAL: Duration = Duration::from_secs(5);

pub(super) struct CancellationWorker {
    state: Arc<WorkerState>,
    _thread: JoinHandle<()>,
}

impl CancellationWorker {
    pub(super) fn start() -> Result<Self, String> {
        let state = Arc::new(WorkerState::default());
        let reconciliation_state = Arc::clone(&state);
        let reconciliation_thread = thread::Builder::new()
            .name("peritus-command-cancellation-reconciliation".to_owned())
            .spawn(move || run_reconciliation_dispatcher(reconciliation_state))
            .map_err(|error| {
                format!("start command cancellation reconciliation worker: {error}")
            })?;
        Ok(Self { state, _thread: reconciliation_thread })
    }

    pub(super) fn enqueue_observer_reconciliation(
        &self,
        runtime: CommandRuntime,
        handle: String,
        control: Option<ProcessControl>,
    ) {
        self.state.enqueue_owned(CancellationTask::new(runtime, handle, control));
    }
}

impl Drop for CancellationWorker {
    fn drop(&mut self) {
        self.state.close();
    }
}

#[derive(Default)]
struct WorkerState {
    reconciliation: TaskLane,
    owners: TaskOwners,
}

impl WorkerState {
    fn enqueue_owned(&self, task: CancellationTask) {
        if !self.owners.admit(&task) {
            return;
        }
        self.reconciliation.enqueue(task);
    }

    fn close(&self) {
        self.reconciliation.close();
    }
}

struct CancellationTask {
    runtime: CommandRuntime,
    handle: String,
    control: Option<ProcessControl>,
    attempts: u64,
    next_report: Instant,
}

impl CancellationTask {
    fn new(
        runtime: CommandRuntime,
        handle: String,
        control: Option<ProcessControl>,
    ) -> Self {
        let now = Instant::now();
        Self {
            runtime,
            handle,
            control,
            attempts: 0,
            next_report: now.checked_add(REPORT_INTERVAL).unwrap_or(now),
        }
    }

    fn reconcile(&self) -> Result<bool, String> {
        // A live C2 owner already retains its execution. Observe again after it publishes
        // a result or exits, including failure and unwind without a terminal result.
        if self.control.as_ref().is_some_and(|control| {
            !control.owner_finished() && control.terminal_result().is_none()
        }) {
            return Ok(false);
        }
        self.runtime
            .reconcile_observer(&self.handle)
            .map_err(|error| error.to_string())
    }

    fn report_pending(&mut self, operation: &'static str, error: Option<&str>) {
        self.attempts = self.attempts.saturating_add(1);
        let now = Instant::now();
        let first_failure = error.is_some() && self.attempts == 1;
        if !first_failure && now < self.next_report {
            return;
        }
        let mode = "observer reconciliation";
        match error {
            Some(error) => crate::diagnostic::report(&format!(
                "peritus command runtime: {mode} {operation} for {} remains owned after attempt {}: {error}",
                self.handle, self.attempts
            )),
            None => crate::diagnostic::report(&format!(
                "peritus command runtime: {mode} {operation} for {} remains owned after attempt {}",
                self.handle, self.attempts
            )),
        }
        self.next_report = now.checked_add(REPORT_INTERVAL).unwrap_or(now);
    }
}

struct TaskLease {
    state: Arc<WorkerState>,
    task: Option<CancellationTask>,
}

impl TaskLease {
    fn new(state: Arc<WorkerState>, task: CancellationTask) -> Self {
        Self { state, task: Some(task) }
    }

    fn task(&self) -> &CancellationTask {
        self.task.as_ref().expect("task lease retains exact work")
    }

    fn task_mut(&mut self) -> &mut CancellationTask {
        self.task.as_mut().expect("task lease retains exact work")
    }

    fn take(&mut self) -> CancellationTask {
        self.task.take().expect("task lease retains exact work")
    }

    fn settled(&mut self) {
        if let Some(task) = self.task.take() {
            self.state.owners.retire(&task);
        }
    }
}

impl Drop for TaskLease {
    fn drop(&mut self) {
        let Some(task) = self.task.take() else { return };
        self.state.reconciliation.enqueue(task);
    }
}

struct ReconciliationSlot {
    state: Arc<WorkerState>,
    task: Mutex<Option<CancellationTask>>,
}

impl Drop for ReconciliationSlot {
    fn drop(&mut self) {
        if let Some(task) = self
            .task
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            self.state.reconciliation.enqueue(task);
        }
    }
}

fn run_reconciliation_dispatcher(state: Arc<WorkerState>) {
    supervise(
        || dispatch_reconciliation_once(&state),
        "cancellation reconciliation dispatcher panicked",
    );
}

fn dispatch_reconciliation_once(state: &Arc<WorkerState>) -> bool {
    let Some(task) = state.reconciliation.take() else { return false };
    let mut lease = TaskLease::new(Arc::clone(state), task);
    let slot = Arc::new(ReconciliationSlot {
        state: Arc::clone(state),
        task: Mutex::new(Some(lease.take())),
    });
    let worker_slot = Arc::clone(&slot);
    match thread::Builder::new()
        .name("peritus-command-cancellation-job".to_owned())
        .spawn(move || run_reconciliation_job(worker_slot))
    {
        Ok(worker) => drop(worker),
        Err(error) => {
            let mut retained = slot.task.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(task) = retained.as_mut() {
                task.report_pending("worker start", Some(&error.to_string()));
            }
            drop(retained);
            thread::sleep(RETRY_INTERVAL);
        }
    }
    true
}

fn supervise(mut step: impl FnMut() -> bool, message: &'static str) {
    loop {
        match catch_unwind(AssertUnwindSafe(&mut step)) {
            Ok(true) => {}
            Ok(false) => return,
            Err(_) => {
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    crate::diagnostic::report(&format!("peritus command runtime: {message}"));
                }));
            }
        }
    }
}

fn run_reconciliation_job(slot: Arc<ReconciliationSlot>) {
    let task = slot
        .task
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    let Some(task) = task else { return };
    let mut lease = TaskLease::new(Arc::clone(&slot.state), task);
    loop {
        match catch_unwind(AssertUnwindSafe(|| lease.task().reconcile())) {
            Ok(Ok(true)) => {
                lease.settled();
                return;
            }
            Ok(Ok(false)) => lease.task_mut().report_pending("settlement", None),
            Ok(Err(error)) => {
                lease.task_mut().report_pending("settlement", Some(&error));
            }
            Err(_) => {
                lease.task_mut().report_pending("settlement", Some("worker panicked"));
            }
        }
        thread::sleep(RETRY_INTERVAL);
    }
}
