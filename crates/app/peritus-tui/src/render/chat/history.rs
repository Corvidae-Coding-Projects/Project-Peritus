//! Read-only message recall: Enter edits a draft; only a subsequent Enter submits it.

use super::{ACCENT, AppModel, MUTED, WARN, wrapped_lines};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph, Wrap},
};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let history = &model.chat.history;
    let Some(search) = &history.search else { return };
    let [heading, body, footer] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(1), Constraint::Length(2)])
            .areas(area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                "Message history",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Line::from(vec![
                Span::styled("Find: ", Style::default().fg(ACCENT)),
                Span::raw(crate::sanitize::sanitize_display_text(&search.query)),
                Span::styled("▏", Style::default().fg(ACCENT)),
            ]),
            Line::default(),
        ]),
        heading,
    );
    let matches = history.matches();
    if matches.is_empty() {
        let message = if history.is_loading() {
            "Searching saved messages…"
        } else {
            "No matching saved messages. Edit the search or Esc to return."
        };
        frame.render_widget(Paragraph::new(message).wrap(Wrap { trim: false }), body);
    } else {
        let items = matches
            .iter()
            .map(|(index, text)| {
                let safe = crate::sanitize::sanitize_display_text(text);
                let mut preview = wrapped_lines(
                    safe.lines().map(|line| Line::raw(line.to_owned())).collect(),
                    usize::from(body.width.saturating_sub(4)).max(1),
                );
                let remaining = preview.len().saturating_sub(3);
                preview.truncate(3);
                if remaining > 0 {
                    preview.push(Line::styled(
                        format!("… {remaining} more lines · Enter loads message"),
                        Style::default().fg(MUTED),
                    ));
                }
                preview.insert(
                    0,
                    Line::styled(
                        format!("Message {}", index + 1),
                        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                    ),
                );
                preview.push(Line::default());
                ListItem::new(preview)
            })
            .collect::<Vec<_>>();
        let mut state = ListState::default();
        state.select(Some(search.selected.min(matches.len() - 1)));
        frame.render_stateful_widget(
            List::new(items)
                .highlight_symbol("▸ ")
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
            body,
            &mut state,
        );
    }
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                "Enter edits draft · Esc returns · ↑ newer · ↓/F3 older",
                Style::default().fg(ACCENT),
            ),
            Line::styled(
                if history.is_loading() {
                    "Loading saved history…"
                } else {
                    "Typing filters messages · recalling never sends or changes them"
                },
                Style::default().fg(WARN),
            ),
        ])
        .wrap(Wrap { trim: false }),
        footer,
    );
}
