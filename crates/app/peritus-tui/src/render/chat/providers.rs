//! Configured provider choices remain available when a retained run's old route is absent.
use super::{
    ACCENT, AppModel, Constraint, Frame, Layout, List, ListItem, ListState, Paragraph, Rect, Style,
    Wrap,
};
pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let regions =
        Layout::vertical([Constraint::Length(3), Constraint::Min(1), Constraint::Length(3)])
            .split(area);
    let role = model.chat.provider_role.map_or("all roles", |role| role.label());
    frame.render_widget(Paragraph::new(format!("Provider for {role}\nApplies to subsequent model turns; saved work and queued instructions are retained.")).wrap(Wrap { trim: false }), regions[0]);
    let items = model.product.as_ref().map_or_else(Vec::new, |product| {
        product
            .launch
            .providers()
            .iter()
            .enumerate()
            .map(|(index, provider)| ListItem::new(format!("{}. {}", index + 1, provider.label())))
            .collect::<Vec<_>>()
    });
    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(model.chat.model_selection.min(items.len() - 1)));
    }
    frame.render_stateful_widget(
        List::new(items).highlight_symbol("▸ ").highlight_style(Style::default().fg(ACCENT)),
        regions[1],
        &mut state,
    );
    frame.render_widget(Paragraph::new("↑/↓ selects · Enter saves · Tab changes role · Esc cancels\nChanged roles use the new provider's configured model. Use /retry to resume a stopped run.").wrap(Wrap { trim: false }), regions[2]);
}
