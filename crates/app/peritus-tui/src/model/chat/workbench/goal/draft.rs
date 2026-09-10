//! Explicit local goal drafting and reviewed confirmation.

use super::{
    AppModel, Effect, GoalDraft, NoticeLevel, RUNNER_CRITERION, WorkbenchExecutionSettings,
    WorkbenchGoalBudget, WorkbenchGoalCriterionDefinition, WorkbenchGoalCriterionKind,
    WorkbenchGoalDefinition, WorkbenchInputState, WorkbenchInputText, WorkbenchIntent,
    WorkbenchQuery,
};

impl AppModel {
    fn graphical_goal_available(&self) -> bool {
        self.features.iter().any(|feature| {
            feature.as_str()
                == peritus_app_protocol::WellKnownProtocolFeature::WorkbenchPreview.as_str()
        })
    }

    pub(super) fn draft_graphical_criterion(&mut self, arguments: &str) -> Vec<Effect> {
        if self.chat.workbench.goal.is_some() || self.chat.workbench.goal_draft.is_none() {
            self.notice(NoticeLevel::Warning,
                "Criteria edit an unconfirmed draft only; use /goal <objective> first. No durable goal changed.");
            return Vec::new();
        }
        let description = if arguments == "remove-graphical" {
            None
        } else if let Some(text) = arguments.strip_prefix("graphical ") {
            if !self.graphical_goal_available() {
                self.notice(
                    NoticeLevel::Warning,
                    "Graphical preview is not negotiated; the goal draft is unchanged.",
                );
                return Vec::new();
            }
            let Ok(text) = WorkbenchInputText::new(text.trim().to_owned()) else {
                self.notice(NoticeLevel::Warning,
                    "Graphical criterion requires 1–8192 bytes without terminal controls; draft unchanged.");
                return Vec::new();
            };
            Some(text)
        } else {
            self.notice(NoticeLevel::Warning,
                "Use /goal criterion graphical <description> or /goal criterion remove-graphical; draft unchanged.");
            return Vec::new();
        };
        if let Some(draft) = &mut self.chat.workbench.goal_draft {
            draft.graphical = description;
        }
        "Draft criterion updated locally; mandatory runner acceptance remains. Review /goal confirm before starting work."
            .clone_into(&mut self.chat.workbench.message);
        self.clear_chat_command();
        Vec::new()
    }

    pub(super) fn draft_goal(&mut self, objective: &str) -> Vec<Effect> {
        if self.chat.workbench.goal.is_some() {
            self.notice(
                NoticeLevel::Warning,
                "This conversation already has a durable goal; it cannot be silently replaced.",
            );
            return Vec::new();
        }
        let Ok(objective) = WorkbenchInputText::new(objective.to_owned()) else {
            self.notice(
                NoticeLevel::Warning,
                "Goal objective must be 1–8192 bytes without terminal controls; draft retained.",
            );
            return Vec::new();
        };
        self.chat.workbench.goal_draft =
            Some(GoalDraft { objective, budget: WorkbenchGoalBudget::default(), graphical: None });
        self.chat.workbench.goal_clear_pending = false;
        self.chat.workbench.brief = None;
        self.chat.workbench.message = format!(
            "Goal drafted with criterion: {RUNNER_CRITERION} Confirmed brief objective must match exactly; /goal confirm starts eligible work."
        );
        self.refresh_brief()
    }

    pub(super) fn confirm_goal(&mut self) -> Vec<Effect> {
        let Some(draft) = self.chat.workbench.goal_draft.clone() else {
            self.notice(
                NoticeLevel::Warning,
                "Draft an objective with /goal <objective> before confirming.",
            );
            return Vec::new();
        };
        if draft.graphical.is_some() && !self.graphical_goal_available() {
            self.notice(NoticeLevel::Warning,
                "Graphical preview is no longer negotiated; no goal started and the draft is retained.");
            return Vec::new();
        }
        if self.chat.workbench.goal.is_some() {
            self.notice(
                NoticeLevel::Warning,
                "This conversation already has a durable goal; no second goal was started.",
            );
            return Vec::new();
        }
        let Some(brief) = self.chat.workbench.brief.as_ref() else {
            self.notice(
                NoticeLevel::Warning,
                "The confirmed brief has not been inspected; draft retained.",
            );
            return self.refresh_brief();
        };
        let matching_objective = brief.entries().iter().any(|entry| {
            entry.field() == peritus_app_protocol::WorkbenchBriefField::Objective
                && entry.source().text() == draft.objective()
                && matches!(
                    entry.source().state(),
                    WorkbenchInputState::Queued | WorkbenchInputState::Incorporated
                )
        });
        if !matching_objective {
            self.notice(
                NoticeLevel::Warning,
                "The draft does not exactly match an eligible user-confirmed brief objective. Use /brief objective <exact text>, then inspect and confirm again.",
            );
            return Vec::new();
        }
        let Some(providers) = self.chat_providers() else {
            self.notice(
                NoticeLevel::Warning,
                "No provider configured. Run peritus providers first; draft retained.",
            );
            return Vec::new();
        };
        let Some(run) = self.ids.run() else { return Vec::new() };
        let criterion = WorkbenchGoalCriterionDefinition::new(
            WorkbenchGoalCriterionKind::RunnerAcceptance,
            WorkbenchInputText::new(RUNNER_CRITERION.to_owned()).expect("static criterion"),
            true,
        );
        let mut criteria = vec![criterion];
        if let Some(description) = draft.graphical {
            criteria.push(WorkbenchGoalCriterionDefinition::new(
                WorkbenchGoalCriterionKind::GraphicalPlaytest,
                description,
                true,
            ));
        }
        let Ok(definition) = WorkbenchGoalDefinition::new(draft.objective, criteria, draft.budget)
        else {
            return Vec::new();
        };
        let settings = WorkbenchExecutionSettings::new(
            run,
            providers,
            self.chat.mode,
            self.chat.models.clone(),
        );
        let Some(workspace) = self.chat.workbench.selected.map(WorkbenchQuery::workspace) else {
            return Vec::new();
        };
        self.submit_workbench(WorkbenchIntent::StartGoal { definition, settings }, workspace)
    }
}
