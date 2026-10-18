//! Scrollable terminal marketplace, retaining the conversation and composer beneath it.
use crate::model::AppModel;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{List, ListItem, ListState, Paragraph, Wrap},
};
pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let Some(menu) = &model.chat.module_menu else { return };
    let regions = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(6),
        Constraint::Length(2),
    ])
    .split(area);
    frame.render_widget(Paragraph::new("Module market · native Rust plugins and workflows\n↑↓ select · r refresh · i install/update · e enable/disable · d remove · Enter use"), regions[0]);
    let items: Vec<_> = menu
        .rows
        .iter()
        .map(|row| {
            ListItem::new(Line::from(format!(
                "{} · {} · {}{}",
                row["name"].as_str().unwrap_or("Unavailable package"),
                if row["installed"] == true {
                    format!(
                        "installed {} / available {}",
                        row["version"].as_str().unwrap_or("?"),
                        row["market_version"].as_str().unwrap_or("—")
                    )
                } else {
                    format!("available {}", row["version"].as_str().unwrap_or("?"))
                },
                if row["enabled"] == true {
                    "enabled"
                } else if row["installed"] == true {
                    "disabled"
                } else {
                    "available"
                },
                if !row["market_digest"].is_null() && row["market_digest"] != row["digest"] {
                    " · update available"
                } else {
                    ""
                }
            )))
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(menu.selected));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_style(Style::default().fg(super::ACCENT).add_modifier(Modifier::BOLD))
            .highlight_symbol("› "),
        regions[1],
        &mut state,
    );
    if let Some(row) = menu.rows.get(menu.selected) {
        frame.render_widget(
            Paragraph::new(crate::sanitize::sanitize_display_text(&format!(
                "{}\n{}\nInstalled/selected digest: {}\nAvailable digest: {}\n{}",
                row["id"].as_str().unwrap_or_default(),
                row["description"].as_str().unwrap_or_default(),
                row["digest"].as_str().unwrap_or_default(),
                row["market_digest"].as_str().unwrap_or("same as selected"),
                if menu.session["module"] == row["id"] && menu.session["digest"] == row["digest"] {
                    menu.session["view"]["actions"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .take(9)
                        .enumerate()
                        .map(|(i, a)| {
                            format!("{} {}", i + 1, a["label"].as_str().unwrap_or_default())
                        })
                        .collect::<Vec<_>>()
                        .join(" · ")
                } else {
                    "Enter to prepare this module's task.".into()
                }
            )))
            .wrap(Wrap { trim: false }),
            regions[2],
        );
    }
    frame.render_widget(
        Paragraph::new(crate::sanitize::sanitize_display_text(&format!(
            "{} · Esc back",
            menu.status
        )))
        .wrap(Wrap { trim: false }),
        regions[3],
    );
}
