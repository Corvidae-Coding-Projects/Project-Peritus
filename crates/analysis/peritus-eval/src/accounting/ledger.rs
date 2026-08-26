//! Canonical complete expected-rollout ledger with unique logical settlement.

use std::collections::BTreeMap;

use crate::{
    EvaluationError, EvaluationErrorKind, EvaluationOperation, EvaluationPlan, EvaluationRecovery,
    RolloutAttempt, RolloutId, RolloutOutcome, RolloutRecord,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct LedgerEntry {
    attempts: Vec<RolloutAttempt>,
    terminal: Option<RolloutRecord>,
}

/// Conserved terminal counts over one complete plan.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LedgerCounts {
    /// Expected logical rollout slots.
    pub expected: u32,
    /// Evaluator-confirmed successes.
    pub passed: u32,
    /// Evaluator-confirmed failures.
    pub task_failed: u32,
    /// Infrastructure terminals without task verdict.
    pub infrastructure_failed: u32,
    /// Durably cancelled logical rollouts.
    pub cancelled: u32,
    /// Ambiguous external outcomes.
    pub ambiguous: u32,
}

impl LedgerCounts {
    /// Returns the complete terminal count without wrapping.
    #[must_use]
    pub fn terminal(self) -> Option<u32> {
        self.passed
            .checked_add(self.task_failed)?
            .checked_add(self.infrastructure_failed)?
            .checked_add(self.cancelled)?
            .checked_add(self.ambiguous)
    }
    /// Returns whether every expected slot has exactly one terminal.
    #[must_use]
    pub fn complete(self) -> bool {
        self.terminal() == Some(self.expected)
    }
}

/// Complete bounded append-only attempt and logical-terminal ledger.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RolloutLedger {
    entries: BTreeMap<RolloutId, LedgerEntry>,
    maximum_attempts: u16,
}

impl RolloutLedger {
    /// Creates one expected entry per rollout in a checked plan.
    #[must_use]
    pub fn from_plan(plan: &EvaluationPlan, maximum_attempts: u16) -> Self {
        let entries = plan
            .specs()
            .iter()
            .map(|spec| (spec.id(), LedgerEntry { attempts: Vec::new(), terminal: None }))
            .collect();
        Self { entries, maximum_attempts }
    }

    /// Retains one monotonic attempt observation.
    ///
    /// # Errors
    /// Rejects unknown rollout, nonmonotonic/conflicting attempt, or configured overflow.
    pub fn record_attempt(
        &mut self,
        rollout: RolloutId,
        attempt: RolloutAttempt,
    ) -> Result<(), EvaluationError> {
        let entry = self.entries.get_mut(&rollout).ok_or_else(unknown)?;
        if attempt.number() > self.maximum_attempts {
            return Err(EvaluationError::new(
                EvaluationErrorKind::LimitExceeded,
                EvaluationOperation::Account,
                EvaluationRecovery::Terminal,
                "rollout attempt exceeds frozen retry policy",
            ));
        }
        if let Some(existing) =
            entry.attempts.iter().find(|value| value.number() == attempt.number())
        {
            return if *existing == attempt {
                Ok(())
            } else {
                Err(conflict("attempt number was retained with different evidence"))
            };
        }
        if entry.attempts.last().is_some_and(|value| value.number() >= attempt.number()) {
            return Err(conflict("rollout attempts are not strictly monotonic"));
        }
        entry.attempts.push(attempt);
        Ok(())
    }

    /// Settles exactly one complete logical terminal record.
    ///
    /// # Errors
    /// Rejects unknown rollout or a conflicting second terminal.
    pub fn settle(&mut self, record: RolloutRecord) -> Result<(), EvaluationError> {
        let rollout = record.rollout_id();
        let entry = self.entries.get_mut(&rollout).ok_or_else(unknown)?;
        match entry.terminal {
            None => {
                entry.terminal = Some(record);
                Ok(())
            }
            Some(existing) if existing == record => Ok(()),
            Some(_) => Err(conflict("logical rollout has conflicting terminal outcomes")),
        }
    }

    /// Returns one logical terminal.
    #[must_use]
    pub fn terminal(&self, rollout: RolloutId) -> Option<RolloutOutcome> {
        self.entries.get(&rollout).and_then(|entry| entry.terminal.map(RolloutRecord::outcome))
    }
    /// Returns one complete terminal record.
    #[must_use]
    pub fn record(&self, rollout: RolloutId) -> Option<RolloutRecord> {
        self.entries.get(&rollout).and_then(|entry| entry.terminal)
    }
    /// Iterates complete terminal records in canonical rollout identity order.
    #[must_use]
    pub fn records(&self) -> std::vec::IntoIter<RolloutRecord> {
        self.entries.values().filter_map(|entry| entry.terminal).collect::<Vec<_>>().into_iter()
    }
    /// Borrows retained attempts for one rollout.
    #[must_use]
    pub fn attempts(&self, rollout: RolloutId) -> Option<&[RolloutAttempt]> {
        self.entries.get(&rollout).map(|entry| entry.attempts.as_slice())
    }
    /// Computes conserved raw counts.
    #[must_use]
    pub fn counts(&self) -> LedgerCounts {
        let mut counts = LedgerCounts {
            expected: u32::try_from(self.entries.len()).unwrap_or(u32::MAX),
            ..LedgerCounts::default()
        };
        for record in self.entries.values().filter_map(|entry| entry.terminal) {
            let counter = match record.outcome() {
                RolloutOutcome::TaskPassed { .. } => &mut counts.passed,
                RolloutOutcome::TaskFailed { .. } => &mut counts.task_failed,
                RolloutOutcome::InfrastructureFailed { .. } => &mut counts.infrastructure_failed,
                RolloutOutcome::Cancelled => &mut counts.cancelled,
                RolloutOutcome::Ambiguous { .. } => &mut counts.ambiguous,
            };
            *counter = counter.saturating_add(1);
        }
        counts
    }
    /// Returns whether the conservation identity proves complete logical settlement.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.counts().complete()
    }
}

const fn unknown() -> EvaluationError {
    crate::invalid(
        EvaluationErrorKind::Binding,
        EvaluationOperation::Account,
        "rollout is absent from the frozen plan",
    )
}

const fn conflict(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Binding,
        EvaluationOperation::Account,
        EvaluationRecovery::Quarantine,
        detail,
    )
}
