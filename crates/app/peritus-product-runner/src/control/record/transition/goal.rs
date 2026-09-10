//! Goal lifecycle, accounting, fork-budget, and evidence transitions.

use crate::control::record::ControlExecution;

use super::{ControlError, ControlIntent, ControlOperation, ConversationRecord};

impl ConversationRecord {
    pub(super) fn apply_execution(
        &mut self,
        operation: &ControlOperation,
        intent: &ControlIntent,
    ) -> Result<(), ControlError> {
        match intent {
            ControlIntent::StartExecution { run, settings_digest } => {
                self.start_execution(operation.id, *run, *settings_digest)
            }
            ControlIntent::StartGoal { .. }
            | ControlIntent::PauseGoal { .. }
            | ControlIntent::ResumeGoal { .. }
            | ControlIntent::UpdateGoalBudget { .. }
            | ControlIntent::ClearGoal { .. } => self.apply_goal_lifecycle(operation.id, intent),
            _ => self.apply_goal_accounting(intent),
        }
    }

    pub(super) fn reserve_fork(
        &mut self,
        branch: &crate::control::ConversationBranch,
        now_unix_millis: u64,
    ) -> Result<(), ControlError> {
        if branch.source() != self.id {
            return Err(ControlError::StaleRevision);
        }
        let checkpoint = self
            .checkpoints
            .iter()
            .find(|value| value.id().as_bytes() == branch.checkpoint().as_bytes())
            .ok_or(ControlError::NotFound)?;
        let references = checkpoint.references();
        let saved = (
            references.source_conversation_revision(),
            references.context_generation(),
            references.brief_revision(),
            references.goal_revision().unwrap_or(0),
        );
        let requested = (
            branch.source_revision(),
            branch.context_generation(),
            branch.brief_revision(),
            branch.goal_revision(),
        );
        if saved != requested {
            return Err(ControlError::InvalidInput);
        }
        match (branch.goal_revision(), branch.allocation()) {
            (0, None) => {}
            (0, Some(_)) | (_, None) => return Err(ControlError::InvalidInput),
            (_, Some(allocation)) => self
                .goal
                .as_mut()
                .ok_or(ControlError::InvalidInput)?
                .reserve_child_budget(allocation, now_unix_millis)?,
        }
        Ok(())
    }

    fn apply_goal_lifecycle(
        &mut self,
        operation: crate::control::OperationId,
        intent: &ControlIntent,
    ) -> Result<(), ControlError> {
        match intent {
            ControlIntent::StartGoal {
                run,
                settings_digest,
                objective,
                criteria,
                budget,
                now_unix_millis,
            } => self.start_goal(
                operation,
                *run,
                *settings_digest,
                objective,
                criteria,
                *budget,
                *now_unix_millis,
            ),
            ControlIntent::PauseGoal { goal, mode, now_unix_millis } => {
                self.goal_mut(*goal)?.pause(*mode, *now_unix_millis)
            }
            ControlIntent::ResumeGoal { goal, now_unix_millis } => {
                self.goal_mut(*goal)?.resume(*now_unix_millis)
            }
            ControlIntent::UpdateGoalBudget { goal, budget, now_unix_millis } => {
                self.goal_mut(*goal)?.update_budget(*budget, *now_unix_millis)
            }
            ControlIntent::ClearGoal { goal, now_unix_millis } => {
                self.goal_mut(*goal)?.cancel(*now_unix_millis)
            }
            _ => Err(ControlError::InvalidInput),
        }
    }

    fn apply_goal_accounting(&mut self, intent: &ControlIntent) -> Result<(), ControlError> {
        match intent {
            ControlIntent::ReserveGoalRequest { goal, role, attempt, now_unix_millis } => {
                let _ = self.goal_mut(*goal)?.reserve_request(*role, *attempt, *now_unix_millis)?;
            }
            ControlIntent::CompleteGoalRequest { goal, role, attempt, report, now_unix_millis } => {
                let _ = self.goal_mut(*goal)?.complete_request(
                    *role,
                    *attempt,
                    *report,
                    *now_unix_millis,
                )?;
            }
            ControlIntent::ReserveGoalTool {
                goal,
                role,
                attempt,
                mutation_capable,
                now_unix_millis,
            } => {
                let _ = self.goal_mut(*goal)?.reserve_tool(
                    *role,
                    *attempt,
                    *mutation_capable,
                    *now_unix_millis,
                )?;
            }
            ControlIntent::CompleteGoalTool { goal, attempt, now_unix_millis } => {
                let _ = self.goal_mut(*goal)?.complete_tool(*attempt, *now_unix_millis)?;
            }
            ControlIntent::ObserveGoalProgress { .. } => self.observe_goal_progress(intent)?,
            ControlIntent::SettleGoal { .. } => self.settle_goal(intent)?,
            ControlIntent::ObserveGraphicalGoalEvidence { .. } => {
                self.observe_graphical_goal_evidence(intent)?;
            }
            _ => return Err(ControlError::InvalidInput),
        }
        Ok(())
    }

    fn observe_goal_progress(&mut self, intent: &ControlIntent) -> Result<(), ControlError> {
        let ControlIntent::ObserveGoalProgress {
            goal,
            attempt,
            elapsed_millis,
            retries,
            provider_failovers,
            compactions,
            workspace_bytes,
            workspace_growth_bytes,
            peak_rss_bytes,
            now_unix_millis,
        } = intent
        else {
            return Err(ControlError::InvalidInput);
        };
        self.goal_mut(*goal)?.observe_progress(
            *attempt,
            *elapsed_millis,
            *retries,
            *provider_failovers,
            *compactions,
            *workspace_bytes,
            *workspace_growth_bytes,
            *peak_rss_bytes,
            *now_unix_millis,
        )
    }

    fn settle_goal(&mut self, intent: &ControlIntent) -> Result<(), ControlError> {
        let ControlIntent::SettleGoal {
            goal,
            attempt,
            settlement,
            evidence_input_generation,
            unresolved_effects,
            now_unix_millis,
        } = intent
        else {
            return Err(ControlError::InvalidInput);
        };
        self.goal_mut(*goal)?.settle(
            *attempt,
            *settlement,
            *evidence_input_generation,
            *unresolved_effects,
            *now_unix_millis,
        )
    }

    fn observe_graphical_goal_evidence(
        &mut self,
        intent: &ControlIntent,
    ) -> Result<(), ControlError> {
        let ControlIntent::ObserveGraphicalGoalEvidence {
            criterion_index,
            goal,
            attempt,
            evidence_user_revision,
            evidence_input_generation,
            now_unix_millis,
            ..
        } = intent
        else {
            return Err(ControlError::InvalidInput);
        };
        self.goal_mut(*goal)?.observe_graphical_evidence(
            *criterion_index,
            *attempt,
            *evidence_user_revision,
            *evidence_input_generation,
            *now_unix_millis,
        )
    }

    fn start_execution(
        &mut self,
        operation: crate::control::OperationId,
        run: [u8; 16],
        settings_digest: [u8; 32],
    ) -> Result<(), ControlError> {
        if self.archived
            || self.execution.is_some()
            || run == [0; 16]
            || self.inputs.capture()?.pending().is_empty()
        {
            return Err(ControlError::InvalidInput);
        }
        self.execution =
            Some(ControlExecution { run, start_operation: operation, settings_digest });
        Ok(())
    }

    #[allow(clippy::too_many_arguments, reason = "one confirmed goal binding remains explicit")]
    fn start_goal(
        &mut self,
        operation: crate::control::OperationId,
        run: [u8; 16],
        settings_digest: [u8; 32],
        objective: &crate::control::ControlText<8192>,
        criteria: &[crate::control::GoalCriterion],
        budget: crate::control::GoalBudget,
        now_unix_millis: u64,
    ) -> Result<(), ControlError> {
        if self.goal.is_some()
            || self
                .brief
                .active_source(crate::control::BriefField::Objective, &self.inputs)
                .is_none_or(|source| source.text() != objective.as_str())
        {
            return Err(ControlError::InvalidInput);
        }
        self.start_execution(operation, run, settings_digest)?;
        self.goal = Some(crate::control::GoalRecord::start(
            operation,
            run,
            objective.clone(),
            criteria.to_vec(),
            budget,
            self.inputs.generation(),
            now_unix_millis,
        )?);
        Ok(())
    }

    fn goal_mut(
        &mut self,
        expected: crate::control::OperationId,
    ) -> Result<&mut crate::control::GoalRecord, ControlError> {
        self.goal.as_mut().filter(|goal| goal.id() == expected).ok_or(ControlError::InvalidInput)
    }
}
