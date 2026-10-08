//! Host-observed request, tool, runner, and graphical evidence transitions.

use super::{
    ControlError, GoalAdmission, GoalAttemptProgress, GoalCriterionKind, GoalCriterionState,
    GoalPauseMode, GoalRecord, GoalRole, GoalSettlement, GoalState, GoalUsageReport,
};
use crate::control::GoalText;

impl GoalRecord {
    pub(in crate::control) fn reserve_request(
        &mut self,
        role: GoalRole,
        attempt: u32,
        now: u64,
    ) -> Result<GoalAdmission, ControlError> {
        let boundary = self.boundary(false, now)?;
        if boundary != GoalAdmission::Accepted {
            return Ok(boundary);
        }
        if attempt != self.attempt {
            return Ok(GoalAdmission::Inactive);
        }
        let usage = &mut self.usage.roles[role.index()];
        usage.requests.increment();
        self.updated_unix_millis = now;
        Ok(GoalAdmission::Accepted)
    }

    pub(in crate::control) fn complete_request(
        &mut self,
        role: GoalRole,
        attempt: u32,
        report: GoalUsageReport,
        now: u64,
    ) -> Result<GoalAdmission, ControlError> {
        if attempt != self.attempt {
            return Ok(GoalAdmission::Inactive);
        }
        let usage = &mut self.usage.roles[role.index()];
        if usage.completed_requests >= usage.requests {
            return Err(ControlError::InvalidInput);
        }
        usage.completed_requests.increment();
        if report.has_tokens() {
            usage.token_reported_requests.increment();
        }
        if report.provider_cost_microunits.is_some() {
            usage.cost_reported_requests.increment();
        }
        usage.input_tokens.add_u64(report.input_tokens.unwrap_or(0));
        usage.cached_input_tokens.add_u64(report.cached_input_tokens.unwrap_or(0));
        usage.output_tokens.add_u64(report.output_tokens.unwrap_or(0));
        if let Some(total) = report.total_tokens {
            usage.total_tokens.add_u64(total);
        } else {
            usage.total_tokens.add_u64(report.input_tokens.unwrap_or(0));
            usage.total_tokens.add_u64(report.output_tokens.unwrap_or(0));
        }
        usage.provider_cost_microunits.add_u64(report.provider_cost_microunits.unwrap_or(0));
        self.updated_unix_millis = now;
        self.boundary(true, now)
    }

    pub(in crate::control) fn reserve_tool(
        &mut self,
        role: GoalRole,
        attempt: u32,
        mutation_capable: bool,
        now: u64,
    ) -> Result<GoalAdmission, ControlError> {
        if self.state == GoalState::Pausing
            && self.pause_mode == Some(GoalPauseMode::BeforeEdit)
            && mutation_capable
        {
            self.mark_paused(now)?;
            return Ok(GoalAdmission::Paused);
        }
        let boundary = self.boundary(false, now)?;
        if boundary != GoalAdmission::Accepted {
            return Ok(boundary);
        }
        if attempt != self.attempt {
            return Ok(GoalAdmission::Inactive);
        }
        let usage = &mut self.usage.roles[role.index()];
        usage.tool_calls.increment();
        self.updated_unix_millis = now;
        Ok(GoalAdmission::Accepted)
    }

    pub(in crate::control) fn complete_tool(
        &mut self,
        attempt: u32,
        now: u64,
    ) -> Result<GoalAdmission, ControlError> {
        if attempt != self.attempt {
            return Ok(GoalAdmission::Inactive);
        }
        self.updated_unix_millis = now;
        self.boundary(true, now)
    }

    #[allow(clippy::too_many_arguments, reason = "runner high-water fields stay explicit")]
    pub(in crate::control) fn observe_progress(
        &mut self,
        attempt: u32,
        elapsed_millis: u64,
        retries: u32,
        provider_failovers: u32,
        compactions: u32,
        workspace_bytes: u64,
        workspace_growth_bytes: u64,
        peak_rss_bytes: u64,
        now: u64,
    ) -> Result<(), ControlError> {
        if attempt != self.attempt {
            return Ok(());
        }
        let previous = self.attempt_progress;
        if elapsed_millis < previous.elapsed_millis
            || retries < previous.retries
            || provider_failovers < previous.provider_failovers
            || compactions < previous.compactions
        {
            return Err(ControlError::InvalidInput);
        }
        self.usage.active_millis.add_u64(elapsed_millis - previous.elapsed_millis);
        self.usage.retries.add_u64(u64::from(retries - previous.retries));
        self.usage
            .provider_failovers
            .add_u64(u64::from(provider_failovers - previous.provider_failovers));
        self.usage.compactions.add_u64(u64::from(compactions - previous.compactions));
        self.usage.workspace_bytes = workspace_bytes;
        self.usage.workspace_growth_bytes =
            self.usage.workspace_growth_bytes.max(workspace_growth_bytes);
        self.usage.peak_rss_bytes = self.usage.peak_rss_bytes.max(peak_rss_bytes);
        self.attempt_progress =
            GoalAttemptProgress { elapsed_millis, retries, provider_failovers, compactions };
        self.updated_unix_millis = now;
        Ok(())
    }

    pub(in crate::control) fn settle(
        &mut self,
        attempt: u32,
        settlement: GoalSettlement,
        evidence_input_generation: Option<u64>,
        unresolved_effects: bool,
        now: u64,
    ) -> Result<(), ControlError> {
        if attempt != self.attempt || matches!(self.state, GoalState::Cancelled) {
            return Ok(());
        }
        if self.state == GoalState::Pausing {
            self.mark_paused(now)?;
            return Ok(());
        }
        let current_acceptance = settlement == GoalSettlement::Accepted
            && evidence_input_generation == Some(self.required_input_generation);
        if current_acceptance {
            for criterion in &mut self.criteria {
                if criterion.kind == GoalCriterionKind::RunnerAcceptance {
                    criterion.state = GoalCriterionState::Satisfied;
                    criterion.evidence_revision = evidence_input_generation;
                }
            }
        }
        match settlement {
            GoalSettlement::Accepted if current_acceptance && !unresolved_effects => {
                if self
                    .criteria
                    .iter()
                    .filter(|criterion| criterion.mandatory)
                    .all(|criterion| criterion.state == GoalCriterionState::Satisfied)
                {
                    self.state = GoalState::Achieved;
                    self.reason = GoalText::new(
                        "Every mandatory current criterion has admissible evidence.".to_owned(),
                    )?;
                } else {
                    self.state = GoalState::WaitingForUser;
                    self.reason = GoalText::new(
                        "Runner acceptance settled, but mandatory external evidence is still missing or unavailable.".to_owned(),
                    )?;
                }
            }
            GoalSettlement::Accepted if current_acceptance => {
                self.state = GoalState::Blocked;
                self.reason = GoalText::new(
                    "Current runner acceptance is retained, but unresolved attempt obligations must be reconciled before resume."
                        .to_owned(),
                )?;
            }
            GoalSettlement::WaitingForUser => {
                self.state = GoalState::WaitingForUser;
                self.reason = GoalText::new(
                    "Execution requires material user input before another attempt.".to_owned(),
                )?;
            }
            GoalSettlement::RecoveryRequired => {
                self.state = GoalState::Blocked;
                self.reason = GoalText::new(
                    "Recovery must reconcile preserved or ambiguous effects before resume."
                        .to_owned(),
                )?;
            }
            GoalSettlement::Cancelled => {
                self.state = GoalState::Cancelled;
                self.reason = GoalText::new(
                    "Execution was cancelled; completed effects and accounting are retained."
                        .to_owned(),
                )?;
            }
            GoalSettlement::Accepted => {
                self.state = GoalState::Blocked;
                self.reason = GoalText::new(
                    "The attempt's acceptance evidence does not bind the current governing input; explicitly resume the retained goal."
                        .to_owned(),
                )?;
            }
            GoalSettlement::Failed => {
                self.state = GoalState::Blocked;
                self.reason = GoalText::new(
                    "The attempt stopped before current runner acceptance; explicitly resume the retained goal and cumulative usage frontier."
                        .to_owned(),
                )?;
            }
        }
        self.updated_unix_millis = now;
        Ok(())
    }

    /// Records only host-verified graphical evidence for the exact current goal revision.
    pub(in crate::control) fn observe_graphical_evidence(
        &mut self,
        criterion_index: u32,
        attempt: u32,
        evidence_user_revision: u64,
        evidence_input_generation: u64,
        now: u64,
    ) -> Result<(), ControlError> {
        if attempt != self.attempt {
            return Ok(());
        }
        if evidence_user_revision != self.user_revision
            || evidence_input_generation != self.required_input_generation
        {
            return Err(ControlError::StaleRevision);
        }
        if self.state == GoalState::Cancelled {
            return Ok(());
        }
        let criterion =
            self.criteria.get_mut(criterion_index as usize).ok_or(ControlError::InvalidInput)?;
        if criterion.kind != GoalCriterionKind::GraphicalPlaytest {
            return Err(ControlError::InvalidInput);
        }
        criterion.state = GoalCriterionState::Satisfied;
        criterion.evidence_revision = Some(evidence_input_generation);
        if self
            .criteria
            .iter()
            .filter(|criterion| criterion.mandatory)
            .all(|criterion| criterion.state == GoalCriterionState::Satisfied)
            && matches!(self.state, GoalState::Active | GoalState::WaitingForUser)
        {
            self.state = GoalState::Achieved;
            self.pause_mode = None;
            self.reason = GoalText::new(
                "Every mandatory current criterion has admissible evidence.".to_owned(),
            )?;
        } else if self.state == GoalState::WaitingForUser {
            self.reason = GoalText::new(
                "Graphical evidence settled, but another mandatory criterion is still missing."
                    .to_owned(),
            )?;
        }
        self.updated_unix_millis = now;
        Ok(())
    }
}
