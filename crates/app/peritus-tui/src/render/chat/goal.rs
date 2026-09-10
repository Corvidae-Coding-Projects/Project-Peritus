//! Truthful goal, criterion, usage, and cumulative-budget projection.

use crate::model::{AppModel, format_id};
use peritus_app_protocol::{
    WorkbenchGoalBudget, WorkbenchGoalCriterionKind, WorkbenchGoalCriterionState,
    WorkbenchGoalPauseMode, WorkbenchGoalRole, WorkbenchGoalState,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let panel = &model.chat.workbench;
    let sections = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let mut lines = vec![Line::from(panel.message.clone())];
    draft_lines(&mut lines, model);
    if let Some(goal) = &panel.goal {
        lines.extend([
            Line::from(format!(
                "Goal {} · run {}",
                format_id(goal.goal().as_bytes()),
                format_id(goal.run().as_bytes())
            )),
            Line::from(format!(
                "State {} · attempt {} · user rev {} · aggregate rev {}",
                state_label(goal.state()),
                goal.attempt(),
                goal.user_revision(),
                goal.aggregate_revision(),
            )),
            Line::from(format!("Objective: {}", goal.objective().as_str())),
            Line::from(format!("Reason: {}", goal.reason())),
            Line::from(format!(
                "Recovery: {}{}",
                if goal.restart_eligible() {
                    "eligible after revalidation"
                } else {
                    "explicit action required"
                },
                goal.pause_mode()
                    .map_or_else(String::new, |mode| format!(" · pending {}", pause_label(mode))),
            )),
        ]);
        for criterion in goal.criteria() {
            lines.push(Line::from(format!(
                "Criterion: {} · {} · {} · evidence {}",
                if criterion.definition().mandatory() { "mandatory" } else { "optional" },
                criterion_kind(criterion.definition().kind()),
                criterion_state(criterion.state()),
                criterion
                    .evidence_revision()
                    .map_or_else(|| "none".to_owned(), |revision| revision.to_string()),
            )));
            lines.push(Line::from(format!("  {}", criterion.definition().description().as_str())));
        }
        let usage = goal.usage();
        lines.extend([
            Line::from(format!("Budget: {}", budget_text(goal.budget()))),
            Line::from(format!(
                "Usage: active {} ms · wall {} ms · requests {} · tools {}",
                usage.active_millis(),
                usage.wall_millis(),
                usage.requests(),
                usage.tool_calls(),
            )),
            Line::from(format!(
                "Totals: tokens {} · provider cost {} microunits · retries {} · failovers {} · compactions {}",
                known(usage.total_tokens()),
                known(usage.provider_cost_microunits()),
                usage.retries(),
                usage.provider_failovers(),
                usage.compactions(),
            )),
            Line::from(format!(
                "Resources: workspace {} B · growth {} B · peak RSS {} B",
                usage.workspace_bytes(),
                usage.workspace_growth_bytes(),
                usage.peak_rss_bytes(),
            )),
        ]);
        for role in usage.roles() {
            lines.push(Line::from(format!(
                "{}: requests {}/{} complete · tools {} · tokens {} · cost {} microunits",
                role_label(role.role()),
                role.completed_requests(),
                role.requests(),
                role.tool_calls(),
                known(role.total_tokens()),
                known(role.provider_cost_microunits()),
            )));
        }
    } else if panel.goal_draft.is_none() {
        lines.push(Line::from("No durable goal is currently loaded."));
    }
    lines.extend([
        Line::from("/pause [now|after-operation|before-edit] · /resume [goal-id]"),
        Line::from("/usage · /budget [time=30m requests=8 tools=32 tokens=100000]"),
        Line::from("Unknown token or cost observations remain unknown, not zero."),
    ]);
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(" Goal · durable execution control "))
        .scroll((u16::try_from(panel.scroll).unwrap_or(u16::MAX), 0));
    frame.render_widget(paragraph, sections[0]);
    frame.render_widget(Paragraph::new("Esc back · ↑↓ scroll · r refresh"), sections[1]);
}

fn draft_lines(lines: &mut Vec<Line<'static>>, model: &AppModel) {
    if let Some(draft) = &model.chat.workbench.goal_draft {
        lines.extend([
            Line::from("Unconfirmed draft · no goal authority yet"),
            Line::from(format!("Objective: {}", draft.objective().as_str())),
            Line::from(format!("Criterion: mandatory · runner acceptance · {}", criterion_text())),
            Line::from(format!("Budget: {}", budget_text(draft.budget()))),
            Line::from(format!(
                "Execution: {} · writer {} · reviewer {} · fixer {}",
                model.chat.mode.label(),
                model_label(model.chat.models.writer()),
                model_label(model.chat.models.reviewer()),
                model_label(model.chat.models.fixer()),
            )),
            Line::from(
                "/goal confirm starts eligible work only after exact brief-objective binding.",
            ),
        ]);
        if let Some(description) = draft.graphical() {
            lines.push(Line::from(format!(
                "Criterion: mandatory · graphical playtest · {}",
                description.as_str()
            )));
        }
        lines.push(Line::from(
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

fn budget_text(budget: WorkbenchGoalBudget) -> String {
    format!(
        "active {} · requests {} · tools {} · tokens {}",
        limit(budget.max_active_millis(), "ms"),
        limit(budget.max_requests(), ""),
        limit(budget.max_tool_calls(), ""),
        limit(budget.max_total_tokens(), ""),
    )
}

fn limit(value: Option<impl std::fmt::Display>, unit: &str) -> String {
    value.map_or_else(|| "host ceiling".to_owned(), |value| format!("{value}{unit}"))
}

fn known(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".to_owned(), |value| value.to_string())
}

const fn state_label(state: WorkbenchGoalState) -> &'static str {
    match state {
        WorkbenchGoalState::Active => "active",
        WorkbenchGoalState::WaitingForUser => "waiting-for-user",
        WorkbenchGoalState::Pausing => "pausing",
        WorkbenchGoalState::Paused => "paused",
        WorkbenchGoalState::Blocked => "blocked",
        WorkbenchGoalState::BudgetReached => "budget-reached",
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
