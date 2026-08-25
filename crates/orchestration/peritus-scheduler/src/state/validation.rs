//! Validation of decoded inert scheduler checkpoints.

use crate::{
    SchedulerError, SchedulerErrorKind, SchedulerPhase, SchedulerState, SchedulerTerminal, WorkId,
    WorkPhase, WorkRecord, WorkTerminal, WorkerPhase,
};

impl SchedulerState {
    pub(crate) fn validate_inert(&self) -> Result<(), SchedulerError> {
        self.binding.validate()?;
        let limits = self.binding.limits();
        if self.work.len() > limits.retained_work() as usize
            || self.workers.len() > usize::from(limits.workers())
            || self.reservations.len() > usize::from(limits.active_reservations())
            || self.used_commands.len() > 65_535
            || self.used_dispatches.len() > 65_535
            || self.estimated_encoded_bytes() > limits.state_bytes()
            || self
                .work
                .iter()
                .filter(|record| {
                    matches!(
                        record.phase(),
                        WorkPhase::Queued
                            | WorkPhase::WaitingDependencies
                            | WorkPhase::RetryPending
                    )
                })
                .count()
                > limits.queued_work() as usize
        {
            return Err(crate::error::reject(
                SchedulerErrorKind::LimitExceeded,
                "decoded scheduler state exceeds immutable bounds",
            ));
        }
        if self
            .workers
            .windows(2)
            .any(|pair| pair[0].descriptor().id() >= pair[1].descriptor().id())
            || self.work.windows(2).any(|pair| pair[0].spec().id() >= pair[1].spec().id())
            || self
                .reservations
                .windows(2)
                .any(|pair| pair[0].dispatch_id() >= pair[1].dispatch_id())
            || self.used_dispatches.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(crate::error::reject(
                SchedulerErrorKind::NonCanonical,
                "decoded scheduler collections are duplicated or noncanonical",
            ));
        }
        let mut commands = self.used_commands.clone();
        commands.sort_unstable();
        commands.dedup();
        if commands.len() != self.used_commands.len() {
            return Err(crate::error::reject(
                SchedulerErrorKind::IdentityConflict,
                "decoded scheduler command identity is duplicated",
            ));
        }
        for record in &self.work {
            self.validate_work_record(record, limits.resource_dimensions())?;
        }
        for reservation in &self.reservations {
            let work = self.work_item(reservation.work_id()).ok_or_else(|| {
                crate::error::reject(
                    SchedulerErrorKind::UnknownIdentity,
                    "decoded reservation work is absent",
                )
            })?;
            let worker = self.worker(reservation.worker_id()).ok_or_else(|| {
                crate::error::reject(
                    SchedulerErrorKind::UnknownIdentity,
                    "decoded reservation worker is absent",
                )
            })?;
            reservation.validate_against(work, worker)?;
            if self.used_dispatches.binary_search(&reservation.dispatch_id()).is_err()
                || (work.phase() == WorkPhase::Reserved && reservation.started())
                || (work.phase() == WorkPhase::Running && !reservation.started())
            {
                return Err(crate::error::reject(
                    SchedulerErrorKind::BindingMismatch,
                    "decoded reservation history, phase, or start acknowledgement differs",
                ));
            }
        }
        self.validate_summary()
    }

    fn validate_work_record(
        &self,
        record: &WorkRecord,
        resource_dimensions: u16,
    ) -> Result<(), SchedulerError> {
        record.spec().request().validate(resource_dimensions)?;
        let limits = self.binding.limits();
        if record.spec().revision() != self.binding.revision()
            || record.spec().maximum_attempts().get() > limits.attempts_per_work()
            || record.attempts_started() > record.spec().maximum_attempts().get()
            || record.bypasses() > limits.bypass_count()
            || (record.phase() == WorkPhase::Terminal) != record.terminal().is_some()
            || (record.phase() == WorkPhase::RetryPending) != record.retry_cause().is_some()
        {
            return Err(crate::error::reject(
                SchedulerErrorKind::BindingMismatch,
                "decoded work record violates revision, attempt, bypass, or phase invariants",
            ));
        }
        for dependency in record.spec().dependencies() {
            if self.work_item(*dependency).is_none() {
                return Err(crate::error::reject(
                    SchedulerErrorKind::UnknownIdentity,
                    "decoded work dependency is absent",
                ));
            }
        }
        if record.spec().parent().is_some_and(|parent| self.work_item(parent).is_none()) {
            return Err(crate::error::reject(
                SchedulerErrorKind::UnknownIdentity,
                "decoded work parent is absent",
            ));
        }
        if has_dependency_cycle(self, record.spec().id()) {
            return Err(crate::error::reject(
                SchedulerErrorKind::InvalidInput,
                "decoded work dependencies contain a cycle",
            ));
        }
        let dependency_status = record.spec().dependencies().iter().fold(
            (true, false),
            |(all_success, failed), dependency| match self
                .work_item(*dependency)
                .and_then(WorkRecord::terminal)
            {
                Some(WorkTerminal::Succeeded { .. }) => (all_success, failed),
                Some(_) => (false, true),
                None => (false, failed),
            },
        );
        if (record.phase() == WorkPhase::Queued && !dependency_status.0)
            || (record.phase() == WorkPhase::WaitingDependencies
                && (dependency_status.0 || dependency_status.1))
        {
            return Err(crate::error::reject(
                SchedulerErrorKind::BindingMismatch,
                "decoded dependency phase differs from prerequisite truth",
            ));
        }
        Ok(())
    }

    fn validate_summary(&self) -> Result<(), SchedulerError> {
        if self.enqueue_ordinal != self.work.len() as u64
            || self.dispatch_ordinal != self.used_dispatches.len() as u64
            || self.sequence.get() != self.used_commands.len() as u64
            || (self.phase == SchedulerPhase::Terminal) != self.terminal.is_some()
        {
            return Err(crate::error::reject(
                SchedulerErrorKind::BindingMismatch,
                "decoded scheduler ordinals, cursor, or terminal phase are inconsistent",
            ));
        }
        for worker in &self.workers {
            if matches!(worker.phase(), WorkerPhase::Lost | WorkerPhase::Removed)
                && self
                    .reservations
                    .iter()
                    .any(|reservation| reservation.worker_id() == worker.descriptor().id())
            {
                return Err(crate::error::reject(
                    SchedulerErrorKind::BindingMismatch,
                    "lost or removed worker retains active ownership",
                ));
            }
        }
        if self
            .terminal
            .as_ref()
            .is_some_and(|terminal| terminal != &SchedulerTerminal::evaluate(&self.work))
            || self.state_digest != crate::canonical::state_digest(self)
        {
            return Err(crate::error::reject(
                SchedulerErrorKind::BindingMismatch,
                "decoded scheduler terminal or complete-state digest is not canonical",
            ));
        }
        if !crate::verified::reservations_fit(self)
            || !crate::verified::unique_dispatch_ownership(self)
            || !crate::verified::no_implicit_success(self)
        {
            return Err(crate::error::reject(
                SchedulerErrorKind::ResourceConflict,
                "decoded scheduler state violates ownership, capacity, or terminal truth",
            ));
        }
        Ok(())
    }
}

fn has_dependency_cycle(state: &SchedulerState, root: WorkId) -> bool {
    let mut frontier =
        state.work_item(root).map_or_else(Vec::new, |record| record.spec().dependencies().to_vec());
    let mut visited = std::collections::BTreeSet::new();
    while let Some(id) = frontier.pop() {
        if id == root {
            return true;
        }
        if visited.insert(id)
            && let Some(record) = state.work_item(id)
        {
            frontier.extend_from_slice(record.spec().dependencies());
        }
    }
    false
}
