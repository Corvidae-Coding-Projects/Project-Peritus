//! Guided agent and model menus; effort is the final page of the same transaction.
use super::{
    ACCENT, AppModel, Constraint, Frame, Layout, Line, List, ListItem, ListState, MUTED, Paragraph,
    Rect, Style, Wrap,
};
use crate::model::chat::{ModelRole, ModelStep};
use peritus_app_protocol::ProductModelEffort;

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    if model.model_setup_step() == Some(ModelStep::Agent) {
        draw_agents(frame, area, model);
        return;
    }
    let regions =
        Layout::vertical([Constraint::Length(4), Constraint::Min(1), Constraint::Length(3)])
            .split(area);
    let current = model.chat.model_role.choice(&model.chat.models);
    frame.render_widget(
        Paragraph::new(format!(
            "2/3 · Choose a model for {}\nCurrent model: {} · effort {}\n{}",
            model.chat.model_role.label(),
            if current.id().is_empty() { "configured model" } else { current.id() },
            current.effort().label(),
            model.chat.catalog.as_ref().map_or("", |c| c.error())
        ))
        .wrap(Wrap { trim: false }),
        regions[0],
    );
    frame.render_widget(Paragraph::new("↑/↓ chooses · Enter next: reasoning effort · r refreshes · Esc back\nThe model and effort are saved together after the final choice.").wrap(Wrap{trim:false}),regions[2]);
    let Some(catalog) = &model.chat.catalog else {
        frame.render_widget(
            Paragraph::new(if model.model_discovery_pending() {
                "Loading this agent's model list…"
            } else {
                "Model list unavailable. Press r to retry or Esc to choose another agent."
            })
            .wrap(Wrap { trim: false }),
            regions[1],
        );
        return;
    };
    let items = catalog
        .models()
        .iter()
        .map(|entry| ListItem::new(format!("{} · {}", entry.label(), entry.id())))
        .collect::<Vec<_>>();
    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(model.model_picker_selection().min(items.len() - 1)));
    }
    frame.render_stateful_widget(
        List::new(items).highlight_symbol("▸ ").highlight_style(Style::default().fg(ACCENT)),
        regions[1],
        &mut state,
    );
}

fn draw_agents(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let regions =
        Layout::vertical([Constraint::Length(3), Constraint::Min(1), Constraint::Length(2)])
            .split(area);
    frame.render_widget(
        Paragraph::new("1/3 · Choose an agent\nSelect whose model and reasoning effort to change."),
        regions[0],
    );
    let roles = [
        (ModelRole::Writer, "Writer — coding and chat"),
        (ModelRole::Reviewer, "Reviewer — independent review"),
        (ModelRole::Fixer, "Fixer — repairs review findings"),
    ];
    let items = roles
        .iter()
        .enumerate()
        .map(|(i, (role, label))| {
            let choice = role.choice(&model.chat.models);
            let effort = if choice.effort() == ProductModelEffort::Default {
                "effort not selected"
            } else {
                choice.effort().label()
            };
            ListItem::new(vec![
                Line::raw(format!("{}. {label}", i + 1)),
                Line::styled(
                    format!(
                        "   {} · {effort}",
                        if choice.id().is_empty() { "configured model" } else { choice.id() }
                    ),
                    Style::default().fg(MUTED),
                ),
                Line::raw(""),
            ])
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default();
    state.select(roles.iter().position(|(r, _)| *r == model.chat.model_role));
    frame.render_stateful_widget(
        List::new(items).highlight_symbol("▸ ").highlight_style(Style::default().fg(ACCENT)),
        regions[1],
        &mut state,
    );
    frame.render_widget(Paragraph::new("↑/↓ chooses · Enter next: model · 1/2/3 selects · Esc cancels\nYour current settings remain until the final save.").wrap(Wrap{trim:false}),regions[2]);
}
