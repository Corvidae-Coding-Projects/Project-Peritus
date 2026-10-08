//! Post-loop cleanup, output closure, and durable terminal publication.

use std::{
    thread,
    time::{Duration, Instant},
};

use crate::{
    EscalationRecord, OsExitObservation, OutputSummary, ProcessError, ProcessEventKind,
    ProcessInstant, TerminalRecovery, TerminalResult, verified::terminal_accounting_valid,
};

use super::SpawnedOwner;
use crate::supervisor::{
    POLL_MILLIS, elapsed_millis, emit,
    finalization::{CompletionFacts, CompletionState, FailureState, classify, convert_exit},
    io::{AccountingSet, drain_input, join_readers, synchronize_spools},
    ownership::ensure_tree_quiescent,
    publish_terminal,
};

impl SpawnedOwner {
    pub(super) fn finish(mut self) -> Result<TerminalResult, ProcessError> {
        if let Some(reason) = self.cancellation.close_admission()
            && self.accept_cancellation(reason).is_err()
        {
            self.failure.owner = true;
        }
        if self.input_task.request_stop().is_err() {
            self.failure.owner = true;
        }
        self.request_resource_sampling_finish();
        self.stop_and_observe_root();
        self.cleanup_tree();
        self.finish_output();
        let output_joined = self.join_output_tasks();
        let input_joined = self.finish_input();
        let tasks_joined = output_joined && input_joined;
        self.release_native();
        self.retire_resource_sampling();
        synchronize_spools(&mut self.spools, &mut self.failure.reader);
        self.failure.owner |= self.failure.reader
            || !self.cleanup.tree_quiescent
            || !self.cleanup.native_released
            || !tasks_joined;
        self.cleanup.complete =
            self.cleanup.tree_quiescent && self.cleanup.native_released && tasks_joined;
        if !self.cleanup.complete {
            return Err(crate::supervisor::supervisor_error(
                "process cleanup remains unresolved after the selected observation interval",
            ));
        }
        if self.failure.reader {
            self.accounting.fail_all();
        }
        let stream_accounting = std::mem::replace(
            &mut self.accounting,
            AccountingSet::new(self.plan.output_policy(), self.plan.io_mode()),
        )
        .finish();
        let observed = checked_stream_total(&stream_accounting, |value| value.observed())?;
        let retained = checked_stream_total(&stream_accounting, |value| value.retained())?;
        let dropped = checked_stream_total(&stream_accounting, |value| value.dropped())?;
        if !terminal_accounting_valid(1, retained, observed, dropped, tasks_joined) {
            self.failure.owner = true;
        }
        self.persist_closed(observed, retained, dropped, tasks_joined);
        if self.cleanup.tree_quiescent {
            emit(&self.shared, &self.plan, None, ProcessEventKind::TreeQuiescent, Vec::new());
        }
        emit(&self.shared, &self.plan, None, ProcessEventKind::OutputClosed, Vec::new());
        let result = self.terminal_result(stream_accounting, observed, tasks_joined);
        self.store.record_terminal(self.plan.process_id(), &result)?;
        publish_terminal(&self.shared, &self.plan, &result);
        Ok(result)
    }

    fn request_resource_sampling_finish(&mut self) {
        self.resources.request_finish(self.tree, &self.plan, &self.shared);
    }

    fn retire_resource_sampling(&mut self) {
        self.resources.retire(&self.plan, &self.shared);
    }

    fn stop_and_observe_root(&mut self) {
        if self.os_exit.is_some() {
            return;
        }
        if self.process.force_kill().is_err() {
            self.failure.owner = true;
        } else if !self.escalation.forced {
            self.escalation.forced = true;
            emit(&self.shared, &self.plan, None, ProcessEventKind::Escalated, Vec::new());
        }
        let began = Instant::now();
        while self
            .plan
            .deadline_policy()
            .reap_millis()
            .is_none_or(|maximum| elapsed_millis(began) < maximum)
        {
            match self.process.try_wait() {
                Ok(Some(exit)) => {
                    if self.observe_exit(convert_exit(&exit)).is_err() {
                        self.failure.owner = true;
                    }
                    return;
                }
                Ok(None) => thread::sleep(Duration::from_millis(POLL_MILLIS)),
                Err(_) => {
                    self.failure.owner = true;
                    return;
                }
            }
        }
        self.failure.owner = true;
    }

    fn cleanup_tree(&mut self) {
        if self.cleanup.tree_quiescent {
            return;
        }
        match ensure_tree_quiescent(
            &mut *self.process,
            self.plan.deadline_policy().reap_millis(),
            &mut self.escalation.forced,
            &self.shared,
            &self.plan,
        ) {
            Ok(quiescent) => self.cleanup.tree_quiescent = quiescent,
            Err(_) => self.failure.owner = true,
        }
        self.failure.owner |= !self.cleanup.tree_quiescent;
    }

    fn finish_output(&mut self) {
        let began = Instant::now();
        while self.eof_count < self.reader_count
            && self
                .plan
                .deadline_policy()
                .reap_millis()
                .is_none_or(|maximum| elapsed_millis(began) < maximum)
        {
            if self.drain_output().is_err() {
                self.failure.reader = true;
            }
            if self.eof_count < self.reader_count {
                thread::sleep(Duration::from_millis(POLL_MILLIS));
            }
        }
        if self.drain_output().is_err() || self.eof_count < self.reader_count {
            self.failure.reader = true;
        }
    }

    fn join_output_tasks(&mut self) -> bool {
        if self.eof_count < self.reader_count {
            self.reader_tasks.clear();
            return false;
        }
        join_readers(std::mem::take(&mut self.reader_tasks), &mut self.failure.reader)
    }

    fn finish_input(&mut self) -> bool {
        while self.input_task.has_task() && !self.input_task.is_finished() {
            if self.input_task.wake_blocked_write().is_err() {
                self.failure.owner = true;
            }
            match drain_input(
                &self.input_task,
                &mut self.input_closed,
                &mut *self.process,
                &self.plan,
                &self.shared,
            ) {
                Ok(_) => {}
                Err(_) => self.failure.owner = true,
            }
            if !self.input_task.is_finished() {
                thread::sleep(Duration::from_millis(POLL_MILLIS));
            }
        }
        let joined = self.input_task.join(&mut self.failure.owner);
        loop {
            match drain_input(
                &self.input_task,
                &mut self.input_closed,
                &mut *self.process,
                &self.plan,
                &self.shared,
            ) {
                Ok(true) => break,
                Ok(false) => {}
                Err(_) => self.failure.owner = true,
            }
        }
        joined
    }

    fn release_native(&mut self) {
        let Some(session) = self.native.as_deref_mut() else {
            self.cleanup.native_released = true;
            return;
        };
        let release = session.release();
        let capture = crate::native::capture_released_session(
            &self.store,
            session,
            &self.plan,
            self.plan.sandbox_digest(),
        );
        let recovery = super::super::publish_native_recovery(&self.shared, session);
        self.cleanup.native_released = release.is_ok() && capture.is_ok() && recovery.is_ok();
        self.failure.owner |= !self.cleanup.native_released;
    }

    fn persist_closed(&mut self, observed: u64, retained: u64, dropped: u64, tasks_joined: bool) {
        let process_id = self.plan.process_id();
        let normal = !self.failure.owner
            && self
                .store
                .record_closed(
                    process_id,
                    observed,
                    retained,
                    dropped,
                    self.cleanup.tree_quiescent,
                    tasks_joined,
                )
                .is_ok();
        if !normal {
            self.failure.owner = true;
            let exit = self.os_exit.clone().unwrap_or(OsExitObservation::Unavailable);
            if self
                .store
                .record_failed_closed(
                    process_id,
                    exit,
                    observed,
                    retained,
                    dropped,
                    self.cleanup.tree_quiescent,
                    tasks_joined,
                )
                .is_err()
            {
                self.failure.owner = true;
            }
        }
    }

    fn terminal_result(
        &self,
        streams: Vec<crate::StreamAccounting>,
        observed: u64,
        tasks_joined: bool,
    ) -> TerminalResult {
        let dropped_events = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .events
            .dropped();
        let exit = self.os_exit.clone().unwrap_or(OsExitObservation::Unavailable);
        let facts = CompletionFacts::new(
            FailureState::from_failed(self.failure.reader),
            CompletionState::from_complete(self.cleanup.tree_quiescent),
            CompletionState::from_complete(tasks_joined),
            FailureState::from_failed(self.failure.owner),
        );
        let disposition = classify(
            self.lifecycle.first_trigger(),
            &exit,
            facts,
            self.resources.limit_exceeded(&self.plan),
        );
        TerminalResult::new(
            self.plan.process_id(),
            self.plan.digest(),
            disposition,
            exit,
            self.lifecycle.first_trigger(),
            EscalationRecord::new(
                self.escalation.graceful_attempted,
                self.escalation.forced,
                self.cleanup.tree_quiescent,
            ),
            self.started_at,
            ProcessInstant::from_millis(elapsed_millis(self.began)),
            OutputSummary::new(streams, dropped_events),
            self.resources.observations(&self.plan, self.began, observed),
            self.cleanup.tree_quiescent,
            tasks_joined,
            TerminalRecovery::OriginalOwner,
        )
    }
}

fn checked_stream_total(
    streams: &[crate::StreamAccounting],
    value: impl Fn(crate::StreamAccounting) -> u64,
) -> Result<u64, ProcessError> {
    streams.iter().try_fold(0_u64, |total, stream| {
        total.checked_add(value(*stream)).ok_or_else(|| {
            crate::supervisor::supervisor_error("aggregate stream accounting overflowed")
        })
    })
}
