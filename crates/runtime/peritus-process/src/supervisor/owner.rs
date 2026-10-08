//! Common post-spawn ownership, cleanup, and terminal-publication funnel.

use std::{
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

use crate::{
    CancellationReason, LifecyclePhase, LifecycleState, OsExitObservation, ProcessError,
    ProcessEventKind, ProcessInstant, ProcessStore, StopStrategy, TerminalResult,
    control::{
        CancellationOwner, ControlCommand, InputLane, InputOwner, SharedObservation,
    },
    output::{RetainedWindow, SpoolSet},
    platform::{PlatformProcess, ProcessTreeIdentity},
};

use super::{
    POLL_MILLIS, SupervisorPlan, elapsed_millis, emit,
    finalization::convert_exit,
    io::{
        AccountingSet, InputTask, accept_trigger, drain_controls, drain_input, drain_output,
        start_readers,
    },
    ownership::ensure_tree_quiescent,
    resource::ResourceTracker,
    supervisor_error,
};

mod finalize;

pub(super) struct SpawnedOwner {
    store: ProcessStore,
    plan: SupervisorPlan,
    shared: Arc<SharedObservation>,
    control_rx: mpsc::Receiver<ControlCommand>,
    cancellation: CancellationOwner,
    process: Box<dyn PlatformProcess>,
    tree: ProcessTreeIdentity,
    input_task: InputTask,
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
    input_closed: bool,
    eof_count: usize,
    escalation: EscalationProgress,
    failure: FailureProgress,
    cleanup: CleanupProgress,
    native: Option<Box<dyn crate::NativeSandboxSession>>,
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
    native_released: bool,
    complete: bool,
}

impl SpawnedOwner {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        store: ProcessStore,
        plan: SupervisorPlan,
        control_rx: mpsc::Receiver<ControlCommand>,
        cancellation: CancellationOwner,
        input_owner: InputOwner,
        input_lane: InputLane,
        shared: Arc<SharedObservation>,
        mut process: Box<dyn PlatformProcess>,
        spools: SpoolSet,
        resources: ResourceTracker,
        began: Instant,
        native: Option<Box<dyn crate::NativeSandboxSession>>,
    ) -> Self {
        let tree = process.identity();
        resources.attach(tree);
        {
            let mut observation =
                shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            observation.tree = Some(tree);
        }
        shared.changed.notify_all();
        let input = process.take_input();
        let (input_task, input_failed) =
            InputTask::start(input, input_owner, input_lane, plan.stdin_policy());
        let readers = process.take_readers();
        let (output_tx, output_rx) = mpsc::sync_channel(super::OUTPUT_QUEUE);
        let readers = start_readers(readers, plan.output_policy(), &output_tx);
        let reader_count = readers.tasks.len();
        let reader_failed = readers.startup_failed;
        drop(output_tx);
        let native_released = native.is_none();
        let input_closed = matches!(plan.stdin_policy(), crate::StdinPolicy::Closed);
        Self {
            store,
            accounting: AccountingSet::new(plan.output_policy(), plan.io_mode()),
            window: RetainedWindow::new(plan.output_policy().retained_window_bytes()),
            lifecycle: LifecycleState::authorized(),
            plan,
            shared,
            control_rx,
            cancellation,
            process,
            tree,
            input_task,
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
            input_closed,
            eof_count: 0,
            escalation: EscalationProgress::default(),
            failure: FailureProgress {
                reader: reader_failed,
                owner: reader_failed || input_failed,
            },
            cleanup: CleanupProgress { tree_quiescent: false, native_released, complete: false },
            native,
        }
    }

    pub(super) fn run(
        mut self,
        initial_failure: bool,
        pre_start_reason: Option<CancellationReason>,
    ) -> Result<TerminalResult, ProcessError> {
        self.failure.owner |= initial_failure;
        let stopped_before_start = pre_start_reason.is_some();
        let startup = if let Some(reason) = pre_start_reason {
            self.begin_pre_start_stop(reason)
        } else if self.failure.owner {
            Ok(())
        } else {
            self.startup()
        };
        if startup.is_err() {
            self.failure.owner = true;
        }
        if stopped_before_start {
            return self.finish();
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

    fn begin_pre_start_stop(
        &mut self,
        fallback_reason: CancellationReason,
    ) -> Result<(), ProcessError> {
        self.lifecycle.advance(LifecyclePhase::Starting)?;
        let reason = self.cancellation.take_pending().unwrap_or(fallback_reason);
        self.accept_cancellation(reason)
    }

    fn startup(&mut self) -> Result<(), ProcessError> {
        self.lifecycle.advance(LifecyclePhase::Starting)?;
        self.store.record_started(self.plan.process_id(), self.tree)?;
        self.lifecycle.advance(LifecyclePhase::Running)?;
        self.started_at = Some(ProcessInstant::from_millis(elapsed_millis(self.began)));
        emit(
            &self.shared,
            &self.plan,
            None,
            ProcessEventKind::Started { root_pid: self.tree.root_pid() },
            Vec::new(),
        );
        self.input_task.activate();
        if self.resources.limit_exceeded(&self.plan)
            || self.resources.sample(self.tree, &self.plan, &self.shared, false)
        {
            self.trigger_resource_limit()?;
        }
        Ok(())
    }

    fn tick(&mut self) -> Result<(), ProcessError> {
        if let Some(reason) = self.cancellation.take_pending() {
            self.accept_cancellation(reason)?;
        }
        if self.lifecycle.first_trigger().is_some() {
            self.input_task.wake_blocked_write()?;
        }
        if self.os_exit.is_none()
            && let Some(session) = self.native.as_deref_mut()
        {
            let poll = session.poll_resources(self.tree);
            let capture = crate::native::capture_activated_session(
                &self.store,
                session,
                &self.plan,
                self.plan.sandbox_digest(),
            );
            let exceeded = poll? == crate::NativePoll::ResourceLimitExceeded;
            capture?;
            super::publish_native_recovery(&self.shared, session)?;
            if exceeded {
                self.trigger_resource_limit()?;
            }
        }
        if self.os_exit.is_none()
            && (self.resources.limit_exceeded(&self.plan)
                || self.resources.sample(self.tree, &self.plan, &self.shared, false))
        {
            self.trigger_resource_limit()?;
        }
        drain_controls(
            &self.control_rx,
            &mut *self.process,
            &self.plan,
            &self.shared,
        )?;
        drain_input(
            &self.input_task,
            &mut self.input_closed,
            &mut *self.process,
            &self.plan,
            &self.shared,
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
            &mut self.escalation.forced,
            &mut *self.process,
            &self.input_task,
            &mut self.native,
        )
    }

    fn trigger_resource_limit(&mut self) -> Result<(), ProcessError> {
        if self.lifecycle.first_trigger().is_none() {
            emit(&self.shared, &self.plan, None, ProcessEventKind::ResourceLimit, Vec::new());
        }
        self.accept_cancellation(CancellationReason::ResourceLimit)
    }

    fn accept_cancellation(&mut self, reason: CancellationReason) -> Result<(), ProcessError> {
        accept_trigger(
            reason,
            &self.plan,
            &self.shared,
            &self.store,
            &mut self.lifecycle,
            &mut self.stopping_at,
            &mut self.escalation.graceful_attempted,
            &mut self.escalation.forced,
            &mut *self.process,
            &self.input_task,
            &mut self.native,
        )
    }

    fn record_admitted_exit_cancellation(
        &mut self,
        reason: CancellationReason,
    ) -> Result<(), ProcessError> {
        if self.lifecycle.first_trigger().is_some()
            || !matches!(self.lifecycle.phase(), LifecyclePhase::Starting | LifecyclePhase::Running)
        {
            return Ok(());
        }
        let sequence = emit(
            &self.shared,
            &self.plan,
            None,
            ProcessEventKind::Cancellation(reason),
            Vec::new(),
        );
        if self.lifecycle.request_stop(sequence, reason) {
            let trigger = self
                .lifecycle
                .first_trigger()
                .expect("the admitted pre-exit cancellation was just recorded");
            self.store.record_stopping(self.plan.process_id(), trigger)?;
        }
        Ok(())
    }

    fn apply_deadline_or_escalation(&mut self) -> Result<(), ProcessError> {
        let wall_limit = match (
            self.plan.deadline_policy().wall_timeout_millis(),
            self.plan.resource_policy().wall_millis(),
        ) {
            (Some(deadline), Some(resource)) => Some(deadline.min(resource)),
            (deadline, None) => deadline,
            (None, resource) => resource,
        };
        if self.os_exit.is_none()
            && self.lifecycle.first_trigger().is_none()
            && wall_limit.is_some_and(|maximum| elapsed_millis(self.began) >= maximum)
        {
            self.accept_cancellation(CancellationReason::Deadline)?;
        }
        if let StopStrategy::GracefulThenForce { escalation_millis, .. } =
            self.plan.deadline_policy().stop_strategy()
            && self.os_exit.is_none()
            && let Some(stopping) = self.stopping_at
            && !self.escalation.forced
            && elapsed_millis(stopping) >= escalation_millis
        {
            self.process.force_kill()?;
            self.escalation.forced = true;
            emit(&self.shared, &self.plan, None, ProcessEventKind::Escalated, Vec::new());
        }
        Ok(())
    }

    fn observe_exit(&mut self, exit: OsExitObservation) -> Result<(), ProcessError> {
        let pending_cancellation = self.cancellation.close_admission();
        let cancellation_result = pending_cancellation.map_or(Ok(()), |reason| {
            // The request was admitted before exit observation closed cancellation admission, so
            // preserve it as the immutable first trigger. The OS has already reported exit: do
            // not invoke backend cancellation or send another process stop.
            self.record_admitted_exit_cancellation(reason)
        });
        self.os_exit = Some(exit.clone());
        let (native_termination, native_capture) = if let Some(session) = self.native.as_deref_mut() {
            let termination = session.terminated(&exit);
            let capture = crate::native::capture_terminated_session(
                &self.store,
                session,
                &self.plan,
                self.plan.sandbox_digest(),
            );
            let recovery = super::publish_native_recovery(&self.shared, session);
            (termination.and(recovery), capture)
        } else {
            (Ok(()), Ok(()))
        };
        let input_stop = self.input_task.request_stop();
        let durable_exit = self.store.record_exit(self.plan.process_id(), exit);
        let lifecycle_exit = self.lifecycle.advance(LifecyclePhase::Exited);
        emit(&self.shared, &self.plan, None, ProcessEventKind::OsExit, Vec::new());
        cancellation_result?;
        native_termination?;
        native_capture?;
        input_stop?;
        durable_exit?;
        lifecycle_exit
    }
}

impl Drop for SpawnedOwner {
    fn drop(&mut self) {
        if !self.cleanup.complete {
            self.cancellation.close_admission();
            let _ = self.input_task.request_stop();
            let _ = self.process.force_kill();
            while self.input_task.has_task() && !self.input_task.is_finished() {
                let _ = self.input_task.wake_blocked_write();
                self.input_task.discard_results();
                thread::sleep(Duration::from_millis(POLL_MILLIS));
            }
            self.input_task.discard_results();
            let _ = self.input_task.join(&mut self.failure.owner);
            if let Some(session) = self.native.as_deref_mut() {
                let _ = session.release();
                let _ = crate::native::capture_released_session(
                    &self.store,
                    session,
                    &self.plan,
                    self.plan.sandbox_digest(),
                );
                let _ = super::publish_native_recovery(&self.shared, session);
            }
        }
    }
}
