//! Common post-spawn ownership, cleanup, and terminal-publication funnel.

use std::{
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

use crate::{
    CancellationReason, ExecutionPlan, LifecyclePhase, LifecycleState, OsExitObservation,
    ProcessError, ProcessEventKind, ProcessInstant, ProcessStore, TerminalResult,
    control::{ControlCommand, SharedObservation},
    output::{RetainedWindow, SpoolSet},
    platform::{PlatformProcess, ProcessTreeIdentity},
};

use super::{
    POLL_MILLIS, elapsed_millis, emit,
    finalization::convert_exit,
    io::{AccountingSet, accept_trigger, drain_controls, drain_output, start_readers},
    ownership::ensure_tree_quiescent,
    resource::ResourceTracker,
    supervisor_error,
};

mod finalize;

pub(super) struct SpawnedOwner {
    store: ProcessStore,
    plan: ExecutionPlan,
    shared: Arc<SharedObservation>,
    control_rx: mpsc::Receiver<ControlCommand>,
    process: Box<dyn PlatformProcess>,
    tree: ProcessTreeIdentity,
    input: Option<Box<dyn std::io::Write + Send>>,
    output_rx: mpsc::Receiver<super::io::ReaderMessage>,
    reader_tasks: Vec<thread::JoinHandle<()>>,
    reader_count: usize,
    spools: SpoolSet,
    accounting: AccountingSet,
    window: RetainedWindow,
    resources: ResourceTracker,
    lifecycle: LifecycleState,
    began: Instant,
    started_at: Option<ProcessInstant>,
    os_exit: Option<OsExitObservation>,
    stopping_at: Option<Instant>,
    total_spooled: u64,
    input_written: u64,
    eof_count: usize,
    escalation: EscalationProgress,
    failure: FailureProgress,
    cleanup: CleanupProgress,
}

#[derive(Default)]
struct EscalationProgress {
    graceful_attempted: bool,
    forced: bool,
}

struct FailureProgress {
    reader: bool,
    owner: bool,
}

#[derive(Default)]
struct CleanupProgress {
    tree_quiescent: bool,
    complete: bool,
}

impl SpawnedOwner {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        store: ProcessStore,
        plan: ExecutionPlan,
        control_rx: mpsc::Receiver<ControlCommand>,
        shared: Arc<SharedObservation>,
        mut process: Box<dyn PlatformProcess>,
        spools: SpoolSet,
        resources: ResourceTracker,
        began: Instant,
    ) -> Self {
        let tree = process.identity();
        let input = process.take_input();
        let readers = process.take_readers();
        let (output_tx, output_rx) = mpsc::sync_channel(super::OUTPUT_QUEUE);
        let readers = start_readers(readers, plan.output_policy(), &output_tx);
        let reader_count = readers.tasks.len();
        let reader_failed = readers.startup_failed;
        drop(output_tx);
        Self {
            store,
            accounting: AccountingSet::new(plan.output_policy(), plan.io_mode()),
            window: RetainedWindow::new(plan.output_policy().retained_window_bytes()),
            lifecycle: LifecycleState::authorized(),
            plan,
            shared,
            control_rx,
            process,
            tree,
            input,
            output_rx,
            reader_tasks: readers.tasks,
            reader_count,
            spools,
            resources,
            began,
            started_at: None,
            os_exit: None,
            stopping_at: None,
            total_spooled: 0,
            input_written: 0,
            eof_count: 0,
            escalation: EscalationProgress::default(),
            failure: FailureProgress { reader: reader_failed, owner: reader_failed },
            cleanup: CleanupProgress::default(),
        }
    }

    pub(super) fn run(mut self, initial_failure: bool) -> Result<TerminalResult, ProcessError> {
        self.failure.owner |= initial_failure;
        if !self.failure.owner && self.startup().is_err() {
            self.failure.owner = true;
        }
        while !self.failure.owner && (self.os_exit.is_none() || self.eof_count < self.reader_count)
        {
            if self.tick().is_err() {
                self.failure.owner = true;
                break;
            }
            thread::sleep(Duration::from_millis(POLL_MILLIS));
        }
        self.finish()
    }

    fn startup(&mut self) -> Result<(), ProcessError> {
        self.lifecycle.advance(LifecyclePhase::Starting)?;
        self.store.record_started(self.plan.identity().process_id(), self.tree)?;
        self.lifecycle.advance(LifecyclePhase::Running)?;
        self.started_at = Some(ProcessInstant::from_millis(elapsed_millis(self.began)));
        emit(
            &self.shared,
            &self.plan,
            None,
            ProcessEventKind::Started { root_pid: self.tree.root_pid() },
            Vec::new(),
        );
        if let Some(process_count) = self.process.process_count()? {
            self.resources.observe_process_count(process_count);
        }
        if self.resources.sample(self.tree, &self.plan, &self.shared, false)? {
            self.trigger_resource_limit()?;
        }
        Ok(())
    }

    fn tick(&mut self) -> Result<(), ProcessError> {
        if self.os_exit.is_none()
            && self.resources.sample(self.tree, &self.plan, &self.shared, false)?
        {
            self.trigger_resource_limit()?;
        }
        drain_controls(
            &self.control_rx,
            &mut *self.process,
            &mut self.input,
            &mut self.input_written,
            &self.plan,
            &self.shared,
            &self.store,
            &mut self.lifecycle,
            &mut self.stopping_at,
            &mut self.escalation.graceful_attempted,
        )?;
        self.drain_output()?;
        if self.os_exit.is_none()
            && let Some(exit) = self.process.try_wait()?
        {
            self.observe_exit(convert_exit(&exit))?;
            self.cleanup.tree_quiescent = ensure_tree_quiescent(
                &mut *self.process,
                self.plan.deadline_policy().reap_millis(),
                &mut self.escalation.forced,
                &self.shared,
                &self.plan,
            )?;
            if !self.cleanup.tree_quiescent {
                return Err(supervisor_error("owned process tree did not become quiescent"));
            }
        }
        self.apply_deadline_or_escalation()?;
        Ok(())
    }

    fn drain_output(&mut self) -> Result<(), ProcessError> {
        drain_output(
            &self.output_rx,
            &mut self.eof_count,
            &mut self.failure.reader,
            &mut self.accounting,
            &mut self.spools,
            &mut self.total_spooled,
            &mut self.window,
            &self.plan,
            &self.shared,
            &self.store,
            &mut self.lifecycle,
            &mut self.stopping_at,
            &mut self.escalation.graceful_attempted,
            &mut *self.process,
            &mut self.input,
        )
    }

    fn trigger_resource_limit(&mut self) -> Result<(), ProcessError> {
        if self.lifecycle.first_trigger().is_none() {
            emit(&self.shared, &self.plan, None, ProcessEventKind::ResourceLimit, Vec::new());
        }
        accept_trigger(
            CancellationReason::ResourceLimit,
            &self.plan,
            &self.shared,
            &self.store,
            &mut self.lifecycle,
            &mut self.stopping_at,
            &mut self.escalation.graceful_attempted,
            &mut *self.process,
            &mut self.input,
        )
    }

    fn apply_deadline_or_escalation(&mut self) -> Result<(), ProcessError> {
        let wall_limit = self
            .plan
            .deadline_policy()
            .wall_timeout_millis()
            .unwrap_or_else(|| self.plan.resource_policy().wall_millis())
            .min(self.plan.resource_policy().wall_millis());
        if self.os_exit.is_none()
            && self.lifecycle.first_trigger().is_none()
            && elapsed_millis(self.began) >= wall_limit
        {
            accept_trigger(
                CancellationReason::Deadline,
                &self.plan,
                &self.shared,
                &self.store,
                &mut self.lifecycle,
                &mut self.stopping_at,
                &mut self.escalation.graceful_attempted,
                &mut *self.process,
                &mut self.input,
            )?;
        }
        if self.os_exit.is_none()
            && let Some(stopping) = self.stopping_at
            && !self.escalation.forced
            && elapsed_millis(stopping) >= self.plan.deadline_policy().grace_millis()
        {
            self.process.force_kill()?;
            self.escalation.forced = true;
            emit(&self.shared, &self.plan, None, ProcessEventKind::Escalated, Vec::new());
        }
        Ok(())
    }

    fn observe_exit(&mut self, exit: OsExitObservation) -> Result<(), ProcessError> {
        self.os_exit = Some(exit.clone());
        self.input.take();
        self.store.record_exit(self.plan.identity().process_id(), exit)?;
        self.lifecycle.advance(LifecyclePhase::Exited)?;
        emit(&self.shared, &self.plan, None, ProcessEventKind::OsExit, Vec::new());
        Ok(())
    }
}

impl Drop for SpawnedOwner {
    fn drop(&mut self) {
        if !self.cleanup.complete {
            self.input.take();
            let _ = self.process.force_kill();
        }
    }
}
