//! Host-observed request, tool, runner, and graphical evidence transitions.

use super::{
    ControlError, ControlText, GoalAdmission, GoalAttemptProgress, GoalCriterionKind,
    GoalCriterionState, GoalPauseMode, GoalRecord, GoalRole, GoalSettlement, GoalState,
    GoalUsageReport,
};

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
        if attempt != self.attempt || !self.next_request_budget_available() {
            if attempt == self.attempt {
                self.reach_budget(now)?;
                return Ok(GoalAdmission::BudgetReached);
            }
            return Ok(GoalAdmission::Inactive);
        }
        let usage = &mut self.usage.roles[role.index()];
        usage.requests = usage.requests.checked_add(1).ok_or(ControlError::Capacity)?;
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
        usage.completed_requests =
            usage.completed_requests.checked_add(1).ok_or(ControlError::Capacity)?;
        if report.has_tokens() {
            usage.token_reported_requests =
                usage.token_reported_requests.checked_add(1).ok_or(ControlError::Capacity)?;
        }
        if report.provider_cost_microunits.is_some() {
            usage.cost_reported_requests =
                usage.cost_reported_requests.checked_add(1).ok_or(ControlError::Capacity)?;
        }
        usage.input_tokens = add(usage.input_tokens, report.input_tokens)?;
        usage.cached_input_tokens = add(usage.cached_input_tokens, report.cached_input_tokens)?;
        usage.output_tokens = add(usage.output_tokens, report.output_tokens)?;
        let derived = report
            .input_tokens
            .unwrap_or(0)
            .checked_add(report.output_tokens.unwrap_or(0))
            .ok_or(ControlError::Capacity)?;
        usage.total_tokens = usage
            .total_tokens
            .checked_add(report.total_tokens.unwrap_or(derived))
            .ok_or(ControlError::Capacity)?;
        usage.provider_cost_microunits =
            add(usage.provider_cost_microunits, report.provider_cost_microunits)?;
        self.updated_unix_millis = now;
        if self.usage.total_tokens().saturating_add(self.child_budget_reservation.total_tokens())
            >= self.budget.effective_tokens()
        {
            self.reach_budget(now)?;
            return Ok(GoalAdmission::BudgetReached);
        }
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
        if attempt != self.attempt
            || self
                .usage
                .tool_calls()
                .checked_add(self.child_budget_reservation.tool_calls())
                .and_then(|used| used.checked_add(1))
                .is_none_or(|next| next > self.budget.effective_tools())
        {
            if attempt == self.attempt {
                self.reach_budget(now)?;
                return Ok(GoalAdmission::BudgetReached);
            }
            return Ok(GoalAdmission::Inactive);
        }
        let usage = &mut self.usage.roles[role.index()];
        usage.tool_calls = usage.tool_calls.checked_add(1).ok_or(ControlError::Capacity)?;
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
        self.usage.active_millis = self
            .usage
            .active_millis
            .checked_add(elapsed_millis - previous.elapsed_millis)
            .ok_or(ControlError::Capacity)?;
        self.usage.retries = self
            .usage
            .retries
            .checked_add(retries - previous.retries)
            .ok_or(ControlError::Capacity)?;
        self.usage.provider_failovers = self
            .usage
            .provider_failovers
            .checked_add(provider_failovers - previous.provider_failovers)
            .ok_or(ControlError::Capacity)?;
        self.usage.compactions = self
            .usage
            .compactions
            .checked_add(compactions - previous.compactions)
            .ok_or(ControlError::Capacity)?;
        self.usage.workspace_bytes = workspace_bytes;
        self.usage.workspace_growth_bytes =
            self.usage.workspace_growth_bytes.max(workspace_growth_bytes);
        self.usage.peak_rss_bytes = self.usage.peak_rss_bytes.max(peak_rss_bytes);
        self.attempt_progress =
            GoalAttemptProgress { elapsed_millis, retries, provider_failovers, compactions };
        self.updated_unix_millis = now;
        if self.usage.active_millis.saturating_add(self.child_budget_reservation.active_millis())
            >= self.budget.effective_active_millis()
        {
            self.reach_budget(now)?;
        }
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
        if self.state == GoalState::BudgetReached {
            self.updated_unix_millis = now;
            return Ok(());
        }
        match settlement {
            GoalSettlement::Accepted
                if !unresolved_effects
                    && evidence_input_generation == Some(self.required_input_generation) =>
            {
                for criterion in &mut self.criteria {
                    if criterion.kind == GoalCriterionKind::RunnerAcceptance {
                        criterion.state = GoalCriterionState::Satisfied;
                        criterion.evidence_revision = evidence_input_generation;
                    }
                }
                if self
                    .criteria
                    .iter()
                    .filter(|criterion| criterion.mandatory)
                    .all(|criterion| criterion.state == GoalCriterionState::Satisfied)
                {
                    self.state = GoalState::Achieved;
                    self.reason = ControlText::new(
                        "Every mandatory current criterion has admissible evidence.".to_owned(),
                    )?;
                } else {
                    self.state = GoalState::WaitingForUser;
                    self.reason = ControlText::new(
                        "Runner acceptance settled, but mandatory external evidence is still missing or unavailable.".to_owned(),
                    )?;
                }
            }
            GoalSettlement::WaitingForUser => {
                self.state = GoalState::WaitingForUser;
                self.reason = ControlText::new(
                    "Execution requires material user input before another attempt.".to_owned(),
                )?;
            }
            GoalSettlement::RecoveryRequired => {
                self.state = GoalState::Blocked;
                self.reason = ControlText::new(
                    "Recovery must reconcile preserved or ambiguous effects before resume."
                        .to_owned(),
                )?;
            }
            GoalSettlement::Cancelled => {
                self.state = GoalState::Cancelled;
                self.reason = ControlText::new(
                    "Execution was cancelled; completed effects and accounting are retained."
                        .to_owned(),
                )?;
            }
            GoalSettlement::Accepted | GoalSettlement::Failed => {
                self.state = GoalState::Blocked;
                self.reason = ControlText::new(
                    "The attempt ended without complete current acceptance evidence.".to_owned(),
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
        if matches!(self.state, GoalState::Cancelled | GoalState::BudgetReached) {
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
            self.reason = ControlText::new(
                "Every mandatory current criterion has admissible evidence.".to_owned(),
            )?;
        } else if self.state == GoalState::WaitingForUser {
            self.reason = ControlText::new(
                "Graphical evidence settled, but another mandatory criterion is still missing."
                    .to_owned(),
            )?;
        }
        self.updated_unix_millis = now;
        Ok(())
    }
}

fn add(current: u64, value: Option<u64>) -> Result<u64, ControlError> {
    current.checked_add(value.unwrap_or(0)).ok_or(ControlError::Capacity)
}
