//! Bounded scrolling for complete prompt choices and authority details.

use super::{ACCENT, BAD, GOOD, MUTED, WARN, field, short_id};
use crate::model::{AppModel, PromptItem, PromptPhase, format_digest, format_id};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(area);
    let items = if model.prompts.is_empty() {
        vec![ListItem::new(Line::styled("No prompts", Style::default().fg(MUTED)))]
    } else {
        model
            .prompts
            .iter()
            .map(|item| {
                let correlation = item.binding.correlation();
                ListItem::new(format!(
                    "{:?}  {:?}  {}",
                    item.binding.kind(),
                    item.phase,
                    short_id(correlation.prompt_id().as_bytes())
                ))
            })
            .collect()
    };
    let mut state = ListState::default();
    if !model.prompts.is_empty() {
        state.select(Some(model.selected_prompt.min(model.prompts.len() - 1)));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL).title(" Awaiting authority/input "))
            .highlight_symbol("▸ ")
            .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        sections[0],
        &mut state,
    );
    let detail = detail_lines(model, sections[1].width);
    let maximum = maximum_scroll(detail.len(), sections[1].height);
    frame.render_widget(
        Paragraph::new(detail)
            .block(Block::default().borders(Borders::ALL).title(" Prompt · PgUp/PgDn · Home/End "))
            .scroll((model.prompt_scroll.min(maximum), 0)),
        sections[1],
    );
}

pub(super) fn scroll_limit(model: &AppModel, area: Rect) -> u16 {
    let sections =
        Layout::horizontal([Constraint::Percentage(38), Constraint::Percentage(62)]).split(area);
    maximum_scroll(detail_lines(model, sections[1].width).len(), sections[1].height)
}

fn maximum_scroll(lines: usize, height: u16) -> u16 {
    u16::try_from(lines.saturating_sub(usize::from(height.saturating_sub(2)))).unwrap_or(u16::MAX)
}

fn detail_lines(model: &AppModel, width: u16) -> Vec<Line<'static>> {
    let text = model
        .selected_prompt_item()
        .map_or_else(|| Text::from("No prompt selected."), prompt_detail);
    super::chat::wrapped_lines(text.lines, usize::from(width.saturating_sub(2)))
}

fn prompt_detail(item: &PromptItem) -> Text<'static> {
    let binding = &item.binding;
    let correlation = binding.correlation();
    let revision = correlation.revision();
    let mut lines = vec![
        field("Kind", format!("{:?}", binding.kind())),
        field("Local phase", format!("{:?}", item.phase)),
        field("Prompt", format_id(correlation.prompt_id().as_bytes())),
        field("Origin request", format_id(correlation.originating_request_id().as_bytes())),
        field("Actor", format_id(correlation.actor_id().as_bytes())),
        field("Workspace", format_id(revision.workspace_id().as_bytes())),
        field(
            "Revision",
            format!(
                "generation {}, revision {}",
                revision.workspace_generation().get(),
                revision.workspace_revision().get()
            ),
        ),
        field("Freshness", format_digest(correlation.freshness_digest().as_bytes())),
    ];
    if let Some(challenge) = binding.approval_challenge() {
        lines
            .push(field("Decision command", format_id(challenge.decision_command_id().as_bytes())));
        lines.push(field("Registry revision", challenge.registry_revision().get().to_string()));
        lines.push(field(
            "Signing challenge",
            format!("{} canonical bytes", challenge.request_frame().len()),
        ));
        lines.push(Line::from(""));
        lines.push(Line::styled(
            "Approval and denial both require an externally signed B1 decision. This client never manufactures authority.",
            Style::default().fg(WARN),
        ));
    }
    if !binding.choices().is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::styled("Choices", Style::default().fg(MUTED)));
        lines.extend(
            binding
                .choices()
                .iter()
                .map(|choice| Line::from(format!("  {} — {}", choice.id(), choice.label()))),
        );
    }
    if !binding.constraints().is_empty() {
        lines.push(Line::from(""));
        lines.push(field("Constraints", format!("{:?}", binding.constraints())));
    }
    lines.push(Line::from(""));
    lines.push(match item.phase {
        PromptPhase::Pending => Line::styled(
            "Enter: respond   c: cancel",
            Style::default().fg(GOOD).add_modifier(Modifier::BOLD),
        ),
        PromptPhase::Submitting => {
            Line::styled("Awaiting daemon response", Style::default().fg(WARN))
        }
        PromptPhase::Accepted => {
            Line::styled("Daemon accepted the protocol input", Style::default().fg(GOOD))
        }
        PromptPhase::Failed => Line::styled(
            "Response not confirmed · Enter retry · c cancel",
            Style::default().fg(BAD),
        ),
    });
    Text::from(lines)
}
