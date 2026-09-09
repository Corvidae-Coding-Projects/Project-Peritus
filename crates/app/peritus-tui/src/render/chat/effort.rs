//! Reasoning effort remains discoverable even when model discovery is unavailable.

use super::{
    ACCENT, AppModel, Constraint, Frame, Layout, List, ListItem, ListState, Paragraph, Rect, Style,
    Wrap,
};
use peritus_app_protocol::ProductModelEffort;

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let regions =
        Layout::vertical([Constraint::Length(3), Constraint::Min(1), Constraint::Length(4)])
            .split(area);
    let choice = model.chat.model_role.choice(&model.chat.models);
    frame.render_widget(
        Paragraph::new(format!(
            "Reasoning effort for {}\nCurrent selection: {}\nModel: {}",
            model.chat.model_role.label(),
            choice.effort().label(),
            if choice.id().is_empty() { "configured model" } else { choice.id() },
        ))
        .wrap(Wrap { trim: false }),
        regions[0],
    );
    let items = ProductModelEffort::ALL
        .into_iter()
        .map(|effort| {
            ListItem::new(if effort == ProductModelEffort::Default {
                "default — high when supported; otherwise no control".to_owned()
            } else {
                effort.label().to_owned()
            })
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default();
    state.select(Some(model.chat.effort_selection.min(items.len() - 1)));
    frame.render_stateful_widget(
        List::new(items).highlight_symbol("▸ ").highlight_style(Style::default().fg(ACCENT)),
        regions[1],
        &mut state,
    );
    frame.render_widget(Paragraph::new(
        "Enter saves · Tab changes role · Esc cancels\nSupport depends on provider/model; unsupported values are not substituted.\nSaved changes apply to the next model turn.",
    ).wrap(Wrap { trim: false }), regions[2]);
}
