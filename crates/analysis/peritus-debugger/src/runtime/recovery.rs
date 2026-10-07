//! Deterministic restart decisions from durable state and owner observations.

use peritus_journal::OutboxDeliveryStatus;
use peritus_policy::AuthorityInstant;

use crate::{
    DebuggerDirectiveDelivery, DebuggerPhase, DebuggerState, ModelDirectiveDelivery,
    ModelRetrySchedule, ModelStartBasis, ModelWorkState, PublicationDirectiveDelivery,
};

/// Closed recovery action selected without performing an effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebuggerRecoveryDecision {
    /// Resume the next deterministic read-only analysis stage.
    ContinueDeterministic,
    /// Claim the durable model directive for this exact attempt.
    ClaimModelAttempt {
        /// Exact one-based attempt to claim.
        attempt: u16,
    },
    /// A live claim fence still owns the exact model directive.
    WaitForModelDirective {
        /// Exact one-based attempt being delivered.
        attempt: u16,
        /// Current positive claim fence.
        fence: u64,
        /// Inclusive tick at which recovery may reclaim the row.
        lease_until: u64,
    },
    /// Reclaim the expired lease for the same model directive and attempt identity.
    ReclaimModelAttempt {
        /// Exact one-based attempt to reclaim.
        attempt: u16,
        /// Expired fence that must no longer settle the directive.
        prior_fence: u64,
        /// Observed elapsed lease boundary.
        lease_until: u64,
    },
    /// Evaluate an epoch-bound schedule against a current authority-clock observation.
    EvaluateModelRetry {
        /// Exact one-based attempt whose directive is pending.
        attempt: u16,
        /// Exact durable schedule to evaluate without resetting its origin.
        schedule: ModelRetrySchedule,
    },
    /// Evaluate an accepted legacy bare-tick directive.
    EvaluateLegacyModelRetry {
        /// Exact one-based attempt whose legacy directive is pending.
        attempt: u16,
        /// Legacy tick threshold, whose epoch and unit were not recorded.
        not_before_tick: u64,
    },
    /// The durable retry directive exists but its delay has not elapsed.
    AwaitModelRetry {
        /// Exact one-based pending attempt.
        attempt: u16,
    },
    /// Reclaim/resume the exact already-started attempt after its C0 lease permits it.
    ResumeModelAttempt {
        /// Exact one-based attempt to resume.
        attempt: u16,
    },
    /// Commit a bounded retry-scheduling transition.
    ScheduleModelRetry {
        /// Exact completed attempt for which a successor must be scheduled.
        completed_attempt: u16,
    },
    /// Assemble, validate, and stage the complete canonical report artifact.
    PrepareReport,
    /// Claim the durable publication directive and admit report evidence.
    ClaimPublication,
    /// A live claim fence still owns the exact publication directive.
    WaitForPublication {
        /// Current positive claim fence.
        fence: u64,
        /// Inclusive tick at which recovery may reclaim the row.
        lease_until: u64,
    },
    /// Reclaim the expired lease for the same publication directive and report identity.
    ReclaimPublication {
        /// Expired fence that must no longer settle the directive.
        prior_fence: u64,
        /// Observed elapsed lease boundary.
        lease_until: u64,
    },
    /// Exact evidence already exists; retry only the publication settlement.
    ReconcilePublication,
    /// Durable terminal work is complete.
    Complete,
    /// Owner observations contradict the durable state and require quarantine.
    Quarantine,
}

/// Chooses the sole restart action from exact state plus read-only C0 owner observations.
///
/// `staged_artifact` means the report digest is finalized and verifies. `evidence_admitted` means
/// the exact content-derived report evidence exists. `directive_available` means the expected
/// durable outbox row is pending/claimable or already held by this recovery worker.
#[must_use]
pub fn decide_recovery(
    state: &DebuggerState,
    staged_artifact: bool,
    evidence_admitted: bool,
    directive_available: bool,
) -> DebuggerRecoveryDecision {
    match state.phase() {
        DebuggerPhase::Created | DebuggerPhase::Selected => {
            DebuggerRecoveryDecision::ContinueDeterministic
        }
        DebuggerPhase::DeterministicComplete | DebuggerPhase::ModelValidated => {
            DebuggerRecoveryDecision::PrepareReport
        }
        DebuggerPhase::ModelPending => match state.model().map(crate::ModelProgress::state) {
            Some(ModelWorkState::Pending { attempt: 1, not_before_tick: 0 })
                if directive_available =>
            {
                DebuggerRecoveryDecision::ClaimModelAttempt { attempt: 1 }
            }
            Some(ModelWorkState::Pending { attempt, not_before_tick }) => {
                DebuggerRecoveryDecision::EvaluateLegacyModelRetry {
                    attempt: *attempt,
                    not_before_tick: *not_before_tick,
                }
            }
            Some(ModelWorkState::PendingOnClock { attempt, schedule }) => {
                DebuggerRecoveryDecision::EvaluateModelRetry {
                    attempt: *attempt,
                    schedule: *schedule,
                }
            }
            Some(ModelWorkState::AwaitingRetry { attempt, .. }) => {
                DebuggerRecoveryDecision::ScheduleModelRetry { completed_attempt: *attempt }
            }
            _ => DebuggerRecoveryDecision::Quarantine,
        },
        DebuggerPhase::ModelRunning => match state.model().map(crate::ModelProgress::state) {
            Some(
                ModelWorkState::Running { attempt, .. }
                | ModelWorkState::RunningOnClock { attempt, .. },
            ) if directive_available => {
                DebuggerRecoveryDecision::ResumeModelAttempt { attempt: *attempt }
            }
            _ => DebuggerRecoveryDecision::Quarantine,
        },
        DebuggerPhase::ReportReady => {
            if !staged_artifact || !directive_available {
                DebuggerRecoveryDecision::Quarantine
            } else if evidence_admitted {
                DebuggerRecoveryDecision::ReconcilePublication
            } else {
                DebuggerRecoveryDecision::ClaimPublication
            }
        }
        DebuggerPhase::Published => {
            if staged_artifact && evidence_admitted {
                DebuggerRecoveryDecision::Complete
            } else {
                DebuggerRecoveryDecision::ReconcilePublication
            }
        }
        DebuggerPhase::Failed | DebuggerPhase::Cancelled => DebuggerRecoveryDecision::Complete,
    }
}

/// Chooses restart work from exact aggregate state and the retained directive delivery state.
///
/// Unlike [`decide_recovery`], this path distinguishes an unclaimed row, a live claim lease, and
/// an expired reclaimable lease. It also validates that the retained directive still names the
/// same job, model attempt, plan/request, or report represented by the aggregate.
#[must_use]
pub fn decide_recovery_with_delivery(
    state: &DebuggerState,
    staged_artifact: bool,
    evidence_admitted: bool,
    delivery: Option<DebuggerDirectiveDelivery>,
) -> DebuggerRecoveryDecision {
    match state.phase() {
        DebuggerPhase::Created | DebuggerPhase::Selected => {
            DebuggerRecoveryDecision::ContinueDeterministic
        }
        DebuggerPhase::DeterministicComplete | DebuggerPhase::ModelValidated => {
            DebuggerRecoveryDecision::PrepareReport
        }
        DebuggerPhase::ModelPending => match state.model().map(crate::ModelProgress::state) {
            Some(ModelWorkState::Pending { attempt: 1, not_before_tick: 0 }) => {
                model_pending_decision(state, 1, delivery)
            }
            Some(ModelWorkState::Pending { attempt, not_before_tick }) => {
                if model_delivery_status(state, *attempt, delivery).is_none() {
                    DebuggerRecoveryDecision::Quarantine
                } else {
                    DebuggerRecoveryDecision::EvaluateLegacyModelRetry {
                        attempt: *attempt,
                        not_before_tick: *not_before_tick,
                    }
                }
            }
            Some(ModelWorkState::PendingOnClock { attempt, schedule }) => {
                if model_delivery_status(state, *attempt, delivery).is_none() {
                    DebuggerRecoveryDecision::Quarantine
                } else {
                    DebuggerRecoveryDecision::EvaluateModelRetry {
                        attempt: *attempt,
                        schedule: *schedule,
                    }
                }
            }
            Some(ModelWorkState::AwaitingRetry { attempt, .. }) => {
                DebuggerRecoveryDecision::ScheduleModelRetry { completed_attempt: *attempt }
            }
            _ => DebuggerRecoveryDecision::Quarantine,
        },
        DebuggerPhase::ModelRunning => match state.model().map(crate::ModelProgress::state) {
            Some(
                ModelWorkState::Running { attempt, .. }
                | ModelWorkState::RunningOnClock { attempt, .. },
            ) => model_running_decision(state, *attempt, delivery),
            _ => DebuggerRecoveryDecision::Quarantine,
        },
        DebuggerPhase::ReportReady => {
            if !staged_artifact {
                return DebuggerRecoveryDecision::Quarantine;
            }
            let Some(status) = publication_delivery_status(state, delivery) else {
                return DebuggerRecoveryDecision::Quarantine;
            };
            match status {
                OutboxDeliveryStatus::Pending if evidence_admitted => {
                    DebuggerRecoveryDecision::ReconcilePublication
                }
                OutboxDeliveryStatus::Pending => DebuggerRecoveryDecision::ClaimPublication,
                OutboxDeliveryStatus::Waiting { fence, lease_until } => {
                    DebuggerRecoveryDecision::WaitForPublication { fence, lease_until }
                }
                OutboxDeliveryStatus::Reclaimable { prior_fence, lease_until } => {
                    DebuggerRecoveryDecision::ReclaimPublication { prior_fence, lease_until }
                }
                OutboxDeliveryStatus::Acknowledged | OutboxDeliveryStatus::Exhausted => {
                    DebuggerRecoveryDecision::Quarantine
                }
            }
        }
        DebuggerPhase::Published => {
            if staged_artifact && evidence_admitted {
                DebuggerRecoveryDecision::Complete
            } else {
                DebuggerRecoveryDecision::ReconcilePublication
            }
        }
        DebuggerPhase::Failed | DebuggerPhase::Cancelled => DebuggerRecoveryDecision::Complete,
    }
}

/// Chooses restart work after evaluating pending retry eligibility on the authority clock.
///
/// This is the execution form of [`decide_recovery`]. It uses the same schedule admission
/// function as the aggregate reducer and claimed-attempt runtime boundary.
#[must_use]
pub fn decide_recovery_on_clock(
    state: &DebuggerState,
    staged_artifact: bool,
    evidence_admitted: bool,
    directive_available: bool,
    observed_at: AuthorityInstant,
) -> DebuggerRecoveryDecision {
    match decide_recovery(
        state,
        staged_artifact,
        evidence_admitted,
        directive_available,
    ) {
        DebuggerRecoveryDecision::EvaluateModelRetry { attempt, schedule } => {
            if schedule.admission(observed_at).is_none() {
                DebuggerRecoveryDecision::AwaitModelRetry { attempt }
            } else if directive_available {
                DebuggerRecoveryDecision::ClaimModelAttempt { attempt }
            } else {
                DebuggerRecoveryDecision::Quarantine
            }
        }
        DebuggerRecoveryDecision::EvaluateLegacyModelRetry {
            attempt,
            not_before_tick,
        } => {
            if observed_at.tick_millis() < not_before_tick {
                DebuggerRecoveryDecision::AwaitModelRetry { attempt }
            } else if directive_available {
                DebuggerRecoveryDecision::ClaimModelAttempt { attempt }
            } else {
                DebuggerRecoveryDecision::Quarantine
            }
        }
        decision => decision,
    }
}

/// Evaluates durable retry scheduling and exact directive wait/reclaim state together.
#[must_use]
pub fn decide_recovery_on_clock_with_delivery(
    state: &DebuggerState,
    staged_artifact: bool,
    evidence_admitted: bool,
    delivery: Option<DebuggerDirectiveDelivery>,
    observed_at: AuthorityInstant,
) -> DebuggerRecoveryDecision {
    match decide_recovery_with_delivery(
        state,
        staged_artifact,
        evidence_admitted,
        delivery,
    ) {
        DebuggerRecoveryDecision::EvaluateModelRetry { attempt, schedule } => {
            if schedule.admission(observed_at).is_none() {
                DebuggerRecoveryDecision::AwaitModelRetry { attempt }
            } else {
                model_pending_decision(state, attempt, delivery)
            }
        }
        DebuggerRecoveryDecision::EvaluateLegacyModelRetry {
            attempt,
            not_before_tick,
        } => {
            if observed_at.tick_millis() < not_before_tick {
                DebuggerRecoveryDecision::AwaitModelRetry { attempt }
            } else {
                model_pending_decision(state, attempt, delivery)
            }
        }
        decision => decision,
    }
}

fn model_pending_decision(
    state: &DebuggerState,
    attempt: u16,
    delivery: Option<DebuggerDirectiveDelivery>,
) -> DebuggerRecoveryDecision {
    match model_delivery_status(state, attempt, delivery) {
        Some(OutboxDeliveryStatus::Pending) => {
            DebuggerRecoveryDecision::ClaimModelAttempt { attempt }
        }
        Some(OutboxDeliveryStatus::Waiting { fence, lease_until }) => {
            DebuggerRecoveryDecision::WaitForModelDirective { attempt, fence, lease_until }
        }
        Some(OutboxDeliveryStatus::Reclaimable { prior_fence, lease_until }) => {
            DebuggerRecoveryDecision::ReclaimModelAttempt {
                attempt,
                prior_fence,
                lease_until,
            }
        }
        Some(OutboxDeliveryStatus::Acknowledged | OutboxDeliveryStatus::Exhausted) | None => {
            DebuggerRecoveryDecision::Quarantine
        }
    }
}

fn model_running_decision(
    state: &DebuggerState,
    attempt: u16,
    delivery: Option<DebuggerDirectiveDelivery>,
) -> DebuggerRecoveryDecision {
    match model_delivery_status(state, attempt, delivery) {
        Some(OutboxDeliveryStatus::Waiting { fence, lease_until }) => {
            DebuggerRecoveryDecision::WaitForModelDirective { attempt, fence, lease_until }
        }
        Some(OutboxDeliveryStatus::Reclaimable { prior_fence, lease_until }) => {
            DebuggerRecoveryDecision::ReclaimModelAttempt {
                attempt,
                prior_fence,
                lease_until,
            }
        }
        Some(
            OutboxDeliveryStatus::Pending
            | OutboxDeliveryStatus::Acknowledged
            | OutboxDeliveryStatus::Exhausted,
        )
        | None => DebuggerRecoveryDecision::Quarantine,
    }
}

fn model_delivery_status(
    state: &DebuggerState,
    attempt: u16,
    delivery: Option<DebuggerDirectiveDelivery>,
) -> Option<OutboxDeliveryStatus> {
    let DebuggerDirectiveDelivery::Model(delivery) = delivery? else {
        return None;
    };
    model_delivery_matches(state, attempt, delivery).then(|| delivery.status())
}

fn model_delivery_matches(
    state: &DebuggerState,
    attempt: u16,
    delivery: ModelDirectiveDelivery,
) -> bool {
    let Some(model) = state.model() else {
        return false;
    };
    let directive = delivery.directive();
    let work_matches = match model.state() {
        ModelWorkState::Pending { attempt: current, not_before_tick } => {
            *current == attempt
                && directive.schedule().is_none()
                && directive.not_before_tick() == *not_before_tick
        }
        ModelWorkState::PendingOnClock { attempt: current, schedule } => {
            *current == attempt && directive.schedule() == Some(*schedule)
        }
        ModelWorkState::Running { attempt: current, .. } => {
            *current == attempt && directive.schedule().is_none()
        }
        ModelWorkState::RunningOnClock {
            attempt: current,
            started_at,
            basis,
            schedule,
        } => {
            let admitted = match (*basis, *schedule) {
                (ModelStartBasis::Immediate, None) => {
                    directive.attempt() == 1 && directive.not_before_tick() == 0
                }
                (ModelStartBasis::LegacyTick, None) => {
                    directive.attempt() > 1
                        && started_at.tick_millis() >= directive.not_before_tick()
                }
                (ModelStartBasis::Scheduled | ModelStartBasis::RestartRebased, Some(value)) => {
                    directive.schedule() == Some(value)
                        && value.admission(*started_at) == Some(*basis)
                }
                _ => false,
            };
            *current == attempt && directive.schedule() == *schedule && admitted
        }
        ModelWorkState::AwaitingRetry { .. }
        | ModelWorkState::Validated { .. }
        | ModelWorkState::Rejected { .. } => false,
    };
    directive.job_id() == state.job_id()
        && directive.model_id() == model.id()
        && directive.attempt() == attempt
        && directive.plan_digest() == model.plan_digest()
        && directive.request_digest() == model.request_digest()
        && work_matches
}

fn publication_delivery_status(
    state: &DebuggerState,
    delivery: Option<DebuggerDirectiveDelivery>,
) -> Option<OutboxDeliveryStatus> {
    let DebuggerDirectiveDelivery::Publication(delivery) = delivery? else {
        return None;
    };
    publication_delivery_matches(state, delivery).then(|| delivery.status())
}

fn publication_delivery_matches(
    state: &DebuggerState,
    delivery: PublicationDirectiveDelivery,
) -> bool {
    let Some(report) = state.report() else {
        return false;
    };
    let directive = delivery.directive();
    directive.job_id() == state.job_id() && directive.report() == report
}
