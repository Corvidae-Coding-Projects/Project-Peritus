//! Explicit persistent-goal confirmation and revision-fenced pause, resume, usage, and budgets.

use super::{AppModel, AppRequestPayload, Effect, NoticeLevel, PendingRequest};
use crate::model::decode_hex_16;
use peritus_app_protocol::{
    ControlOperationId, WellKnownProtocolFeature, WorkbenchExecutionSettings, WorkbenchGoalBudget,
    WorkbenchGoalCriterionDefinition, WorkbenchGoalCriterionKind, WorkbenchGoalDefinition,
    WorkbenchGoalPauseMode, WorkbenchGoalSnapshot, WorkbenchInputState, WorkbenchInputText,
    WorkbenchIntent, WorkbenchQuery,
};

mod budget;
mod draft;
use budget::parse_budget;

const RUNNER_CRITERION: &str = "Pass the existing strict runner acceptance gate.";

/// Local, unconfirmed definition. It has no daemon authority until `/goal confirm` is receipted.
#[derive(Clone, Debug)]
pub struct GoalDraft {
    objective: WorkbenchInputText,
    budget: WorkbenchGoalBudget,
    graphical: Option<WorkbenchInputText>,
}

impl GoalDraft {
    pub(crate) const fn objective(&self) -> &WorkbenchInputText {
        &self.objective
    }
    pub(crate) const fn budget(&self) -> WorkbenchGoalBudget {
        self.budget
    }

    pub(crate) const fn graphical(&self) -> Option<&WorkbenchInputText> {
        self.graphical.as_ref()
    }
}

impl AppModel {
    pub(in crate::model::chat) fn goal_command(&mut self, arguments: &str) -> Vec<Effect> {
        if !self.goal_available() {
            self.notice(
                NoticeLevel::Warning,
                "Persistent goals unavailable/offline; /reconnect or upgrade daemon. Draft retained.",
            );
            return Vec::new();
        }
        if self.chat.workbench.selected.is_none() {
            self.notice(
                NoticeLevel::Warning,
                "Select a conversation with /sessions first; draft retained.",
            );
            return Vec::new();
        }
        self.open_goal_panel();
        if self.workbench_request_pending() || self.chat.workbench.unresolved.is_some() {
            self.notice(
                NoticeLevel::Warning,
                "Resolve the pending workbench request before changing the goal; draft retained.",
            );
            return Vec::new();
        }
        match arguments {
            "" => {
                self.chat.workbench.goal = None;
                self.chat.workbench.goal_clear_pending = false;
                "Reading the durable goal and cumulative accounting; no inference."
                    .clone_into(&mut self.chat.workbench.message);
                self.refresh_goal()
            }
            "confirm" => self.confirm_goal(),
            "criterion" => self.draft_graphical_criterion(""),
            value if value.starts_with("criterion ") => {
                self.draft_graphical_criterion(value[10..].trim())
            }
            "clear" => {
                let Some(goal) = self.chat.workbench.goal.as_ref() else {
                    return self.inspect_goal_before("clearing it");
                };
                self.chat.workbench.goal_clear_pending = true;
                self.chat.workbench.message = format!(
                    "Clear will cancel future continuation of goal {} at a safe boundary; history and completed effects remain. Run /goal clear confirm.",
                    crate::model::format_id(goal.goal().as_bytes())
                );
                Vec::new()
            }
            "clear confirm" => self.clear_goal(),
            objective => self.draft_goal(objective),
        }
    }

    pub(in crate::model::chat) fn pause_goal_command(&mut self, arguments: &str) -> Vec<Effect> {
        if !self.prepare_goal_control() {
            return Vec::new();
        }
        let mode = match arguments {
            "" | "after-operation" => WorkbenchGoalPauseMode::AfterOperation,
            "now" => WorkbenchGoalPauseMode::Now,
            "before-edit" => WorkbenchGoalPauseMode::BeforeEdit,
            _ => {
                self.notice(
                    NoticeLevel::Warning,
                    "Use /pause [now | after-operation | before-edit]; draft retained.",
                );
                return Vec::new();
            }
        };
        let Some((goal, workspace)) = self.current_goal_binding() else {
            return self.inspect_goal_before("pausing it");
        };
        self.submit_workbench(WorkbenchIntent::PauseGoal { goal, mode }, workspace)
    }

    pub(in crate::model::chat) fn resume_goal_command(&mut self, arguments: &str) -> Vec<Effect> {
        if !self.prepare_goal_control() {
            return Vec::new();
        }
        let Some((goal, workspace)) = self.current_goal_binding() else {
            return self.inspect_goal_before("resuming it");
        };
        if !arguments.is_empty() {
            let Some(requested) =
                decode_hex_16(arguments).and_then(|bytes| ControlOperationId::new(bytes).ok())
            else {
                self.notice(
                    NoticeLevel::Warning,
                    "Use /resume or /resume <32 hexadecimal goal ID>; draft retained.",
                );
                return Vec::new();
            };
            if requested != goal {
                self.notice(
                    NoticeLevel::Warning,
                    "That goal ID is not the inspected goal; no work resumed.",
                );
                return Vec::new();
            }
        }
        self.submit_workbench(WorkbenchIntent::ResumeGoal { goal }, workspace)
    }

    pub(in crate::model::chat) fn usage_command(&mut self) -> Vec<Effect> {
        if !self.prepare_goal_inspection() {
            return Vec::new();
        }
        self.chat.workbench.goal = None;
        "Reading cumulative usage; unavailable provider counters remain unknown."
            .clone_into(&mut self.chat.workbench.message);
        self.refresh_goal()
    }

    pub(in crate::model::chat) fn budget_command(&mut self, arguments: &str) -> Vec<Effect> {
        if !self.prepare_goal_inspection() {
            return Vec::new();
        }
        if arguments.is_empty() {
            self.chat.workbench.goal = None;
            "Reading cumulative limits and usage; no budget changed."
                .clone_into(&mut self.chat.workbench.message);
            return self.refresh_goal();
        }
        if self.workbench_request_pending() || self.chat.workbench.unresolved.is_some() {
            self.notice(
                NoticeLevel::Warning,
                "Resolve the pending workbench request before changing the budget; draft retained.",
            );
            return Vec::new();
        }
        let current = self
            .chat
            .workbench
            .goal
            .as_ref()
            .map(WorkbenchGoalSnapshot::budget)
            .or_else(|| self.chat.workbench.goal_draft.as_ref().map(GoalDraft::budget));
        let Some(current) = current else {
            return self.inspect_goal_before("editing its budget, or draft a new goal first");
        };
        let budget = match parse_budget(arguments, current) {
            Ok(budget) => budget,
            Err(message) => {
                self.notice(NoticeLevel::Warning, message);
                return Vec::new();
            }
        };
        if let Some(draft) = &mut self.chat.workbench.goal_draft
            && self.chat.workbench.goal.is_none()
        {
            draft.budget = budget;
            "Draft limits updated locally; /goal confirm is still required."
                .clone_into(&mut self.chat.workbench.message);
            return Vec::new();
        }
        let Some((goal, workspace)) = self.current_goal_binding() else {
            return self.inspect_goal_before("editing its budget");
        };
        self.submit_workbench(WorkbenchIntent::UpdateGoalBudget { goal, budget }, workspace)
    }

    pub(super) fn refresh_goal(&mut self) -> Vec<Effect> {
        if !self.goal_available() || self.workbench_request_pending() {
            return Vec::new();
        }
        let Some(query) = self.chat.workbench.selected else { return Vec::new() };
        self.request(
            AppRequestPayload::QueryWorkbenchGoal(query),
            PendingRequest::WorkbenchGoal(query),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model) fn accept_workbench_goal(
        &mut self,
        query: WorkbenchQuery,
        goal: WorkbenchGoalSnapshot,
    ) {
        if goal.query() != query
            || self.chat.workbench.selected != Some(query)
            || !self.chat.workbench.goal_mode
        {
            return;
        }
        self.chat.workbench.goal = Some(goal);
        self.chat.workbench.goal_clear_pending = false;
        self.chat.workbench.scroll = 0;
        self.chat.workbench.message.clear();
    }

    fn clear_goal(&mut self) -> Vec<Effect> {
        if !self.chat.workbench.goal_clear_pending {
            self.notice(
                NoticeLevel::Warning,
                "Inspect /goal clear before confirming its safe-boundary cancellation effect.",
            );
            return Vec::new();
        }
        let Some((goal, workspace)) = self.current_goal_binding() else {
            return self.inspect_goal_before("clearing it");
        };
        self.submit_workbench(WorkbenchIntent::ClearGoal { goal }, workspace)
    }

    fn prepare_goal_control(&mut self) -> bool {
        if !self.prepare_goal_inspection() {
            return false;
        }
        if self.workbench_request_pending() || self.chat.workbench.unresolved.is_some() {
            self.notice(
                NoticeLevel::Warning,
                "Resolve the pending workbench request before controlling the goal; draft retained.",
            );
            return false;
        }
        true
    }

    fn prepare_goal_inspection(&mut self) -> bool {
        if !self.goal_available() {
            self.notice(
                NoticeLevel::Warning,
                "Persistent goals unavailable/offline; /reconnect or upgrade daemon. Draft retained.",
            );
            return false;
        }
        if self.chat.workbench.selected.is_none() {
            self.notice(
                NoticeLevel::Warning,
                "Select a conversation with /sessions first; draft retained.",
            );
            return false;
        }
        self.open_goal_panel();
        true
    }

    const fn open_goal_panel(&mut self) {
        self.chat.workbench.goal_mode = true;
        self.chat.workbench.mode = super::WorkbenchMode::Sessions;
        self.chat.workbench.context_mode = None;
        self.chat.workbench.images.open = false;
        self.chat.workbench.files.open = false;
        self.chat.workbench.open = true;
    }

    fn current_goal_binding(&self) -> Option<(ControlOperationId, peritus_types::WorkspaceId)> {
        let goal = self.chat.workbench.goal.as_ref()?;
        (self.chat.workbench.selected == Some(goal.query()))
            .then_some((goal.goal(), goal.query().workspace()))
    }

    fn inspect_goal_before(&mut self, action: &str) -> Vec<Effect> {
        self.chat.workbench.message = format!(
            "Reading the current goal before {action}; repeat the command after inspection."
        );
        self.refresh_goal()
    }

    fn goal_available(&self) -> bool {
        self.workbench_available()
            && self.features.iter().any(|feature| {
                feature.as_str() == WellKnownProtocolFeature::WorkbenchGoals.as_str()
            })
    }
}
