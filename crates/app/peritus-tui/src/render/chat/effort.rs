//! Reasoning effort remains discoverable even when model discovery is unavailable.

use super::{
    ACCENT, AppModel, Constraint, Frame, Layout, List, ListItem, ListState, Paragraph, Rect, Style,
    Wrap,
};
use crate::model::chat::ModelStep;
use peritus_app_protocol::ProductModelEffort;

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let regions =
        Layout::vertical([Constraint::Length(4), Constraint::Min(1), Constraint::Length(4)])
            .split(area);
    let guided = model.model_setup_step() == Some(ModelStep::Effort);
    let choice = model
        .model_setup_choice()
        .unwrap_or_else(|| model.chat.model_role.choice(&model.chat.models));
    frame.render_widget(
        Paragraph::new(format!(
            "{} for {}\nCurrent selection: {}\nModel: {}",
            if guided { "3/3 · Choose reasoning effort" } else { "Reasoning effort" },
            model.chat.model_role.label(),
            if choice.effort() == ProductModelEffort::Default {
                "not selected"
            } else {
                choice.effort().label()
            },
            if choice.id().is_empty() { "configured model" } else { choice.id() },
        ))
        .wrap(Wrap { trim: false }),
        regions[0],
    );
    let items = ProductModelEffort::ALL
        .into_iter()
        .map(|effort| {
            ListItem::new(if guided && effort == ProductModelEffort::Default {
                "No reasoning effort (non-reasoning models only)".to_owned()
            } else if effort == ProductModelEffort::Default {
                "not selected — choose an effort before sending".to_owned()
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
    frame.render_widget(Paragraph::new(if guided {
        "↑/↓ chooses · Enter saves model and effort · Esc back\nReasoning models require an explicit effort; unsupported values are not substituted.\nSaved changes apply to the next model turn."
    }else {
        "Enter saves · Tab changes role · Esc cancels\nSupport depends on provider/model; unsupported values are not substituted.\nSaved changes apply to the next model turn; no effort is chosen automatically."
    },
    ).wrap(Wrap { trim: false }), regions[2]);
}
