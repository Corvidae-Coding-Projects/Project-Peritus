//! Truthful goal, criterion, and usage projection.

use crate::model::{AppModel, format_id};
use peritus_app_protocol::{
    WorkbenchGoalAmount,
    WorkbenchGoalCriterionKind, WorkbenchGoalCriterionState, WorkbenchGoalPauseMode,
    WorkbenchGoalRole, WorkbenchGoalState,
};
use ratatui::{Frame, layout::Rect};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    super::inspector::draw(
        frame,
        area,
        model,
        content(model),
        " Goal · durable execution control ",
        "Esc back · ↑↓/PgUp/PgDn scroll · Home/End · r refresh",
    );
}

pub(super) fn content(model: &AppModel) -> Vec<String> {
    let panel = &model.chat.workbench;
    let mut lines = vec![panel.message.clone()];
    draft_lines(&mut lines, model);
    if let Some(goal) = &panel.goal {
        lines.extend([
            format!(
                "Goal {} · run {}",
                format_id(goal.goal().as_bytes()),
                format_id(goal.run().as_bytes())
            ),
            format!(
                "State {} · attempt {} · user rev {} · aggregate rev {}",
                state_label(goal.state()),
                goal.attempt(),
                goal.user_revision(),
                goal.aggregate_revision(),
            ),
            format!("Objective: {}", goal.objective().as_str()),
            format!("Reason: {}", goal.reason()),
            format!(
                "Recovery: {}{}",
                if goal.restart_eligible() {
                    "eligible after revalidation"
                } else {
                    "explicit action required"
                },
                goal.pause_mode()
                    .map_or_else(String::new, |mode| format!(" · pending {}", pause_label(mode))),
            ),
        ]);
        for criterion in goal.criteria() {
            lines.push(format!(
                "Criterion: {} · {} · {} · evidence {}",
                if criterion.definition().mandatory() { "mandatory" } else { "optional" },
                criterion_kind(criterion.definition().kind()),
                criterion_state(criterion.state()),
                criterion
                    .evidence_revision()
                    .map_or_else(|| "none".to_owned(), |revision| revision.to_string()),
            ));
            lines.push(format!("  {}", criterion.definition().description().as_str()));
        }
        let usage = goal.usage();
        lines.extend([
            format!(
                "Usage: active {} ms · wall {} ms · requests {} · tools {}",
                usage.active_millis(),
                usage.wall_millis(),
                usage.requests(),
                usage.tool_calls(),
            ),
            format!(
                "Totals: tokens {} · provider cost {} microunits · retries {} · failovers {} · compactions {}",
                known(usage.total_tokens()),
                known(usage.provider_cost_microunits()),
                usage.retries(),
                usage.provider_failovers(),
                usage.compactions(),
            ),
            format!(
                "Resources: workspace {} B · growth {} B · peak RSS {} B",
                usage.workspace_bytes(),
                usage.workspace_growth_bytes(),
                usage.peak_rss_bytes(),
            ),
        ]);
        for role in usage.roles() {
            lines.push(format!(
                "{}: requests {}/{} complete · tools {} · tokens {} · cost {} microunits",
                role_label(role.role()),
                role.completed_requests(),
                role.requests(),
                role.tool_calls(),
                known(role.total_tokens()),
                known(role.provider_cost_microunits()),
            ));
        }
    } else if panel.goal_draft.is_none() {
        lines.push(String::from("No durable goal is currently loaded."));
    }
    lines.extend([
        String::from("/pause [now|after-operation|before-edit] · /resume [goal-id]"),
        String::from("/usage"),
        String::from("Unknown token or cost observations remain unknown, not zero."),
    ]);
    lines
}

fn draft_lines(lines: &mut Vec<String>, model: &AppModel) {
    if let Some(draft) = &model.chat.workbench.goal_draft {
        lines.extend([
            String::from("Unconfirmed draft · no goal authority yet"),
            format!("Objective: {}", draft.objective().as_str()),
            format!("Criterion: mandatory · runner acceptance · {}", criterion_text()),
            format!(
                "Execution: {} · writer {} · reviewer {} · fixer {}",
                model.chat.mode.label(),
                model_label(model.chat.models.writer()),
                model_label(model.chat.models.reviewer()),
                model_label(model.chat.models.fixer()),
            ),
            String::from(
                "/goal confirm saves this objective in the brief, then starts eligible work.",
            ),
        ]);
        if let Some(description) = draft.graphical() {
            lines.push(format!(
                "Criterion: mandatory · graphical playtest · {}",
                description.as_str()
            ));
        }
        lines.push(String::from(
            "/goal criterion graphical <description> · criterion remove-graphical",
        ));
    }
}

const fn criterion_text() -> &'static str {
    "Pass the existing strict runner acceptance gate."
}

fn model_label(choice: &peritus_app_protocol::ProductModelChoice) -> &str {
    if choice.id().is_empty() { "configured model" } else { choice.id() }
}

fn known(value: Option<WorkbenchGoalAmount>) -> String {
    value.map_or_else(|| "unknown".to_owned(), |value| value.to_string())
}

const fn state_label(state: WorkbenchGoalState) -> &'static str {
    match state {
        WorkbenchGoalState::Active => "active",
        WorkbenchGoalState::WaitingForUser => "waiting-for-user",
        WorkbenchGoalState::Pausing => "pausing",
        WorkbenchGoalState::Paused => "paused",
        WorkbenchGoalState::Blocked => "blocked",
        WorkbenchGoalState::Achieved => "achieved",
        WorkbenchGoalState::Cancelled => "cancelled",
    }
}

const fn pause_label(mode: WorkbenchGoalPauseMode) -> &'static str {
    match mode {
        WorkbenchGoalPauseMode::Now => "now",
        WorkbenchGoalPauseMode::AfterOperation => "after-operation",
        WorkbenchGoalPauseMode::BeforeEdit => "before-edit",
    }
}

const fn criterion_kind(kind: WorkbenchGoalCriterionKind) -> &'static str {
    match kind {
        WorkbenchGoalCriterionKind::RunnerAcceptance => "runner acceptance",
        WorkbenchGoalCriterionKind::GraphicalPlaytest => "graphical playtest",
        WorkbenchGoalCriterionKind::HumanValidation => "human validation",
    }
}

const fn criterion_state(state: WorkbenchGoalCriterionState) -> &'static str {
    match state {
        WorkbenchGoalCriterionState::Pending => "pending",
        WorkbenchGoalCriterionState::Satisfied => "satisfied",
        WorkbenchGoalCriterionState::Unavailable => "unavailable",
        WorkbenchGoalCriterionState::Stale => "stale",
    }
}

const fn role_label(role: WorkbenchGoalRole) -> &'static str {
    match role {
        WorkbenchGoalRole::Writer => "Writer",
        WorkbenchGoalRole::Reviewer => "Reviewer",
        WorkbenchGoalRole::Fixer => "Fixer",
    }
}
