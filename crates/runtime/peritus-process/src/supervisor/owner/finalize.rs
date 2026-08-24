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
    io::{AccountingSet, join_readers, synchronize_spools},
    ownership::ensure_tree_quiescent,
    publish_terminal,
};

impl SpawnedOwner {
    pub(super) fn finish(mut self) -> Result<TerminalResult, ProcessError> {
        self.input.take();
        self.sample_before_cleanup();
        self.stop_and_observe_root();
        self.cleanup_tree();
        self.finish_output();
        let tasks_joined = self.join_output_tasks();
        synchronize_spools(&mut self.spools, &mut self.failure.reader);
        self.failure.owner |= self.failure.reader || !self.cleanup.tree_quiescent || !tasks_joined;
        if self.failure.reader {
            self.accounting.fail_all();
        }
        let stream_accounting = std::mem::replace(
            &mut self.accounting,
            AccountingSet::new(self.plan.output_policy(), self.plan.io_mode()),
        )
        .finish();
        let observed = stream_accounting.iter().map(|value| value.observed()).sum::<u64>();
        let retained = stream_accounting.iter().map(|value| value.retained()).sum::<u64>();
        let dropped = stream_accounting.iter().map(|value| value.dropped()).sum::<u64>();
        if !terminal_accounting_valid(1, retained, observed, dropped, tasks_joined) {
            self.failure.owner = true;
        }
        self.persist_closed(observed, retained, dropped, tasks_joined);
        if self.cleanup.tree_quiescent {
            emit(&self.shared, &self.plan, None, ProcessEventKind::TreeQuiescent, Vec::new());
        }
        emit(&self.shared, &self.plan, None, ProcessEventKind::OutputClosed, Vec::new());
        let result = self.terminal_result(stream_accounting, observed, tasks_joined);
        self.cleanup.complete = self.cleanup.tree_quiescent && tasks_joined;
        self.store.record_terminal(self.plan.identity().process_id(), &result)?;
        publish_terminal(&self.shared, &self.plan, &result);
        Ok(result)
    }

    fn sample_before_cleanup(&mut self) {
        if self.resources.sample(self.tree, &self.plan, &self.shared, true).is_err() {
            self.failure.owner = true;
        }
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
        while elapsed_millis(began) < self.plan.deadline_policy().reap_millis() {
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
            && elapsed_millis(began) < self.plan.deadline_policy().reap_millis()
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

    fn persist_closed(&mut self, observed: u64, retained: u64, dropped: u64, tasks_joined: bool) {
        let process_id = self.plan.identity().process_id();
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
            self.plan.identity().process_id(),
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
