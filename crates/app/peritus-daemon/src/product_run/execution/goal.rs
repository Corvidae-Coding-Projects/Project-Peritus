//! Exact runner terminal settlement projected into the cumulative goal ledger.

use peritus_product_runner::control::{
    ControlError, GoalCriterionKind, GoalCriterionState, GoalState,
};

use super::{
    GoalSettlement, ProductRunOutcome, ProductRunService, ProductRunServiceError, RunDisposition,
};

struct PendingGoalAccounting {
    start: peritus_product_runner::control::ControlOperation,
    cancellation: peritus_journal::JournalCancellation,
    authoritative_revision: u64,
    observation: crate::product_control::GoalProgressObservation,
    prepared: Option<crate::product_control::PreparedGoalProgress>,
}

pub(super) struct PreparedWorkbenchGoal {
    start: peritus_product_runner::control::ControlOperation,
    reconciliation: crate::product_control::ControlReconciliation,
    prepared: Option<crate::product_control::PreparedGoalSettlement>,
    binding: Option<crate::product_control::GoalSemanticBinding>,
    settlement: GoalSettlement,
    unresolved_effects: bool,
    evidence_input_generation: Option<u64>,
}

impl PreparedWorkbenchGoal {
    pub(super) fn binding(&self) -> Option<&crate::product_control::GoalSemanticBinding> {
        self.binding.as_ref()
    }

    pub(super) const fn settlement_tag(&self) -> u16 {
        self.settlement as u16
    }

    pub(super) const fn unresolved_effects(&self) -> bool {
        self.unresolved_effects
    }

    pub(super) const fn evidence_input_generation(&self) -> Option<u64> {
        self.evidence_input_generation
    }
}

impl ProductRunService {
    pub(super) fn prepare_recovery_workbench_goal(
        &self,
        record: &super::super::RunRecord,
    ) -> Result<PreparedWorkbenchGoal, ProductRunServiceError> {
        let evidence_input_generation =
            (record.interaction.incorporated > 0).then_some(record.interaction.incorporated);
        self.prepare_bound_workbench_goal(
            record,
            GoalSettlement::RecoveryRequired as u16,
            evidence_input_generation,
            true,
        )
    }

    pub(super) fn prepare_bound_workbench_goal(
        &self,
        record: &super::super::RunRecord,
        settlement_tag: u16,
        evidence_input_generation: Option<u64>,
        unresolved_effects: bool,
    ) -> Result<PreparedWorkbenchGoal, ProductRunServiceError> {
        let start = record.interaction.workbench.clone();
        let settlement = match settlement_tag {
            value if value == GoalSettlement::Accepted as u16 => GoalSettlement::Accepted,
            value if value == GoalSettlement::WaitingForUser as u16 => {
                GoalSettlement::WaitingForUser
            }
            value if value == GoalSettlement::RecoveryRequired as u16 => {
                GoalSettlement::RecoveryRequired
            }
            value if value == GoalSettlement::Cancelled as u16 => GoalSettlement::Cancelled,
            value if value == GoalSettlement::Failed as u16 => GoalSettlement::Failed,
            _ => return Err(ProductRunServiceError::InvalidMessage),
        };
        let reconciliation =
            self.retained_control_reconciliation(record.request.run_id(), &record.cancelled)?;
        let prepared = self.with_control_reconciliation(&reconciliation, |store| {
            store.prepare_goal_settlement(
                &start,
                settlement,
                evidence_input_generation,
                unresolved_effects,
            )
        })?;
        let binding = match prepared.as_ref() {
            Some(prepared) => Some(prepared.binding().clone()),
            None => self.with_control_reconciliation(&reconciliation, |store| {
                store.inspect_goal_semantic(&start)
            })?,
        };
        Ok(PreparedWorkbenchGoal {
            start,
            reconciliation,
            prepared,
            binding,
            settlement,
            unresolved_effects,
            evidence_input_generation,
        })
    }

    pub(super) fn prepare_workbench_goal(
        &self,
        record: &super::super::RunRecord,
        result: &Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>,
    ) -> Result<PreparedWorkbenchGoal, ProductRunServiceError> {
        let start = record.interaction.workbench.clone();
        let evidence_input_generation =
            (record.interaction.incorporated > 0).then_some(record.interaction.incorporated);
        let accepted = result.as_ref().is_ok_and(|outcome| {
            outcome.settlement().disposition() == RunDisposition::Accepted
        });
        let user_cancelled = record.user_cancelled
            || self.user_cancellation_requested(record.request.run_id());
        let (settlement, unresolved_effects) = if user_cancelled && !accepted {
            (GoalSettlement::Cancelled, true)
        } else if self.inner.control_shutdown.is_cancelled() && !accepted {
            (GoalSettlement::RecoveryRequired, true)
        } else {
            match result {
                Ok(outcome) => {
                    let settlement = match outcome.settlement().disposition() {
                        RunDisposition::Accepted => GoalSettlement::Accepted,
                        RunDisposition::WaitingForUser => GoalSettlement::WaitingForUser,
                        RunDisposition::RecoveryRequired => GoalSettlement::RecoveryRequired,
                        RunDisposition::Cancelled => GoalSettlement::Cancelled,
                        RunDisposition::CandidateAvailable | RunDisposition::FailedNoCandidate => {
                            GoalSettlement::Failed
                        }
                    };
                    let unresolved =
                        outcome.settlement().disposition() != RunDisposition::Accepted
                            || outcome.candidate().is_none()
                            || outcome.settlement().checkpoint().is_none()
                            || !outcome.remaining_work().is_empty();
                    (settlement, unresolved)
                }
                Err(error)
                    if error.kind()
                        == peritus_product_runner::ProductRunnerErrorKind::Cancelled =>
                {
                    (GoalSettlement::Cancelled, true)
                }
                Err(_) => (GoalSettlement::Failed, true),
            }
        };
        let reconciliation =
            self.retained_control_reconciliation(record.request.run_id(), &record.cancelled)?;
        let prepared = self.with_control_reconciliation(&reconciliation, |store| {
            if accepted
                && store.capture_execution(&start)?.inputs().generation()
                    != evidence_input_generation.ok_or(ControlError::StaleRevision)?
            {
                return Err(ControlError::StaleRevision.into());
            }
            store.prepare_goal_settlement(
                &start,
                settlement,
                evidence_input_generation,
                unresolved_effects,
            )
        })?;
        let binding = match prepared.as_ref() {
            Some(prepared) => Some(prepared.binding().clone()),
            None => self.with_control_reconciliation(&reconciliation, |store| {
                store.inspect_goal_semantic(&start)
            })?,
        };
        Ok(PreparedWorkbenchGoal {
            start,
            reconciliation,
            prepared,
            binding,
            settlement,
            unresolved_effects,
            evidence_input_generation,
        })
    }

    pub(super) fn commit_workbench_goal(
        &self,
        prepared: PreparedWorkbenchGoal,
        accepted: bool,
        evidence_input_generation: Option<u64>,
    ) -> Result<Option<&'static str>, ProductRunServiceError> {
        let PreparedWorkbenchGoal {
            start,
            reconciliation,
            prepared,
            binding: _,
            settlement: _,
            unresolved_effects: _,
            evidence_input_generation: _,
        } = prepared;
        if let Some(prepared) = prepared {
            self.with_control_reconciliation(&reconciliation, |store| {
                store.commit_goal_settlement(prepared)
            })?;
        }
        self.with_control_reconciliation(&reconciliation, |store| {
            let record = store.load(start.conversation())?;
            if accepted
                && record.as_ref().and_then(|record| record.goal()).is_some_and(|goal| {
                    !goal.criteria().iter().any(|criterion| {
                        criterion.kind() == GoalCriterionKind::RunnerAcceptance
                            && criterion.state() == GoalCriterionState::Satisfied
                            && criterion.evidence_revision() == evidence_input_generation
                    })
                })
            {
                return Err(ControlError::StaleRevision.into());
            }
            Ok(record.as_ref().and_then(|record| record.goal()).and_then(|goal| {
                match goal.state() {
                    GoalState::Paused => {
                        Some("Paused — completed effects retained; /resume continues")
                    }
                    _ => None,
                }
            }))
        })
        .map_err(Into::into)
    }
}

impl ProductRunService {
    pub(super) async fn with_goal_clock<F>(
        &self,
        run: peritus_types::RunId,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        recovery: std::sync::Arc<crate::product_control::ControlReconciliation>,
        execution: F,
    ) -> Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>
    where
        F: Future<Output = Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>>,
    {
        let started = std::time::Instant::now();
        tokio::pin!(execution);
        let mut stopped = false;
        let mut tick_millis = 100;
        let mut governing_unavailable = None;
        let mut pending = None;
        loop {
            tokio::select! {
                result = &mut execution => {
                    if !stopped {
                        loop {
                            match self.deliver_goal_accounting(
                                run,
                                &cancelled,
                                &recovery,
                                started.elapsed(),
                                &mut pending,
                            ) {
                                Ok(_) => {
                                    if governing_unavailable.take().is_some() {
                                        crate::diagnostic::report(&format!(
                                            "peritusd: goal progress for {run:?} recovered under its retained control owner",
                                        ));
                                    }
                                    break;
                                }
                                Err(ProductRunServiceError::GoverningStateUnavailable(
                                    unavailable,
                                )) if !cancelled.load(
                                    std::sync::atomic::Ordering::Acquire,
                                ) => {
                                    if governing_unavailable.is_none() {
                                        crate::diagnostic::report(&format!(
                                            "peritusd: {unavailable}; final goal progress is suspended while the exact owner retries",
                                        ));
                                    }
                                    governing_unavailable = Some(unavailable);
                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                }
                                Err(ProductRunServiceError::GoverningStateUnavailable(_)) => {
                                    break;
                                }
                                Err(error) => {
                                    crate::diagnostic::report(&format!(
                                        "peritusd: goal accounting delivery for {run:?} remains recoverable under its original operation: {error}",
                                    ));
                                    break;
                                }
                            }
                        }
                    }
                    return result;
                }
                () = tokio::time::sleep(std::time::Duration::from_millis(tick_millis)), if !stopped => {
                    match self.deliver_goal_accounting(
                        run,
                        &cancelled,
                        &recovery,
                        started.elapsed(),
                        &mut pending,
                    ) {
                        Err(ProductRunServiceError::GoverningStateUnavailable(unavailable)) => {
                            if cancelled.load(std::sync::atomic::Ordering::Acquire) {
                                stopped = true;
                            } else if governing_unavailable.is_none() {
                                crate::diagnostic::report(&format!(
                                    "peritusd: {unavailable}; goal progress is suspended while the exact owner retries",
                                ));
                                governing_unavailable = Some(unavailable);
                                tick_millis = 50;
                            } else {
                                governing_unavailable = Some(unavailable);
                                tick_millis = 50;
                            }
                        }
                        Err(error) => {
                            crate::diagnostic::report(&format!(
                                "peritusd: goal accounting delivery for {run:?} is deferred under its original operation while execution remains active: {error}",
                            ));
                            stopped = true;
                        }
                        Ok(true) => {
                            if governing_unavailable.take().is_some() {
                                crate::diagnostic::report(&format!(
                                    "peritusd: goal progress for {run:?} recovered under its retained control owner",
                                ));
                            }
                            tick_millis = 1000;
                        }
                        Ok(false) => stopped = true,
                    }
                }
            }
        }
    }

    fn deliver_goal_accounting(
        &self,
        run: peritus_types::RunId,
        attempt: &std::sync::Arc<std::sync::atomic::AtomicBool>,
        recovery: &std::sync::Arc<crate::product_control::ControlReconciliation>,
        elapsed: std::time::Duration,
        pending: &mut Option<PendingGoalAccounting>,
    ) -> Result<bool, ProductRunServiceError> {
        if pending.is_none() {
            let records =
                self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
            let record = records.get(&run).ok_or(ProductRunServiceError::NotFound)?;
            if record.control_cancellation.is_cancelled()
                || attempt.load(std::sync::atomic::Ordering::Acquire)
            {
                return Ok(false);
            }
            pending.replace(PendingGoalAccounting {
                start: record.interaction.workbench.clone(),
                cancellation: record.control_cancellation.clone(),
                authoritative_revision: record.interaction.incorporated,
                observation: crate::product_control::GoalProgressObservation::new(
                    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
                    record.progress.retries,
                    record.progress.provider_failovers,
                    record.progress.compactions,
                    record.progress.workspace_bytes,
                    record.progress.workspace_growth_bytes,
                    record.progress.peak_rss_bytes,
                ),
                prepared: None,
            });
        }
        let delivery = pending.as_mut().ok_or(ProductRunServiceError::Unavailable)?;
        if delivery.cancellation.is_cancelled()
            || attempt.load(std::sync::atomic::Ordering::Acquire)
        {
            *pending = None;
            return Ok(false);
        }
        if delivery.prepared.is_none() {
            let prepared = self.with_control_reconciliation(recovery, |store| {
                store.prepare_goal_progress(&delivery.start, delivery.observation)
            });
            match prepared {
                Ok(Some(prepared)) => delivery.prepared = Some(prepared),
                Ok(None) => {
                    *pending = None;
                    return Ok(false);
                }
                Err(error)
                    if matches!(
                        &error,
                        crate::product_control::ControlStoreError::Journal(_)
                            | crate::product_control::ControlStoreError::Io(_)
                            | crate::product_control::ControlStoreError::Corrupt(_)
                    ) => {
                        return Err(ProductRunServiceError::GoverningStateUnavailable(
                            super::super::GoverningStateUnavailable::new(
                                run,
                                delivery.start.clone(),
                                delivery.authoritative_revision,
                                "prepare durable goal accounting delivery",
                                error,
                                std::sync::Arc::clone(recovery),
                            ),
                        ));
                    }
                Err(error) => return Err(error.into()),
            }
        }
        let prepared = delivery.prepared.as_ref().ok_or(ProductRunServiceError::Unavailable)?;
        let result = self.with_control_reconciliation(recovery, |store| {
            if delivery.cancellation.is_cancelled()
                || attempt.load(std::sync::atomic::Ordering::Acquire)
            {
                return Err(crate::product_control::ControlStoreError::ContentionCancelled);
            }
            store.commit_goal_progress(prepared)
        });
        match result {
            Ok(()) => {
                *pending = None;
                Ok(true)
            }
            Err(crate::product_control::ControlStoreError::ContentionCancelled)
                if delivery.cancellation.is_cancelled()
                    || attempt.load(std::sync::atomic::Ordering::Acquire) => {
                *pending = None;
                Ok(false)
            }
            Err(error)
                if matches!(
                    &error,
                    crate::product_control::ControlStoreError::Journal(_)
                        | crate::product_control::ControlStoreError::Io(_)
                        | crate::product_control::ControlStoreError::Corrupt(_)
                ) => Err(ProductRunServiceError::GoverningStateUnavailable(
                    super::super::GoverningStateUnavailable::new(
                        run,
                        delivery.start.clone(),
                        delivery.authoritative_revision,
                        "commit durable goal accounting delivery",
                        error,
                        std::sync::Arc::clone(recovery),
                    ),
                )),
            result => result.map_err(Into::into),
        }
    }
}
