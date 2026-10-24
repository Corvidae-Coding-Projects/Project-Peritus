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
        Constraint::Length(5),
        Constraint::Length(12),
        Constraint::Length(2),
    ])
    .split(area);
    frame.render_widget(Paragraph::new("Module market · native Rust plugins and workflows\n↑↓ package · ←→/Tab action · 1–9 shortcut · Enter run/use · r refresh · b build · x cancel · e enable · d remove"), regions[0]);
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
                } else if row["source"] == true {
                    format!("source {}", row["version"].as_str().unwrap_or("?"))
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
                "{}\n{}\nInstalled/selected digest: {}\nAvailable digest: {}",
                row["id"].as_str().unwrap_or_default(),
                row["description"].as_str().unwrap_or_default(),
                row["digest"].as_str().unwrap_or("not installed"),
                row["market_digest"]
                    .as_str()
                    .or_else(|| row["source_digest"].as_str())
                    .unwrap_or("same as selected"),
            )))
            .wrap(Wrap { trim: false }),
            regions[2],
        );
        let session = &menu.session;
        let details = if session["module"] == row["id"] && session["digest"] == row["digest"] {
            let view = &session["view"];
            let actions = view["actions"].as_array();
            let action = actions
                .and_then(|actions| actions.get(menu.selected_action))
                .map(|action| {
                    format!(
                        "Action {}/{}: {} → /module {} {}",
                        menu.selected_action + 1,
                        actions.map_or(0, Vec::len),
                        action["label"].as_str().unwrap_or("Unnamed action"),
                        row["id"].as_str().unwrap_or_default(),
                        action["arguments"].as_str().unwrap_or_default(),
                    )
                })
                .unwrap_or_else(|| "No module actions have been published yet.".into());
            let progress = format!(
                "Progress: {} — {}",
                view["progress"]["state"].as_str().unwrap_or("unknown"),
                view["progress"]["summary"].as_str().unwrap_or("No concrete operation published."),
            );
            let export = view["artifact"].as_object().map_or_else(
                || "Export: none selected".into(),
                |_| {
                    format!(
                        "Export: {} · {} bytes · {}\n{}",
                        view["artifact"]["name"].as_str().unwrap_or("module artifact"),
                        view["artifact"]["bytes"].as_u64().unwrap_or(0),
                        view["artifact"]["sha256"].as_str().unwrap_or("unknown digest"),
                        view["artifact"]["export_path"].as_str().unwrap_or("path unavailable"),
                    )
                },
            );
            let summary = view["summary"].as_str().unwrap_or("No durable module summary yet.");
            format!("{progress}\n{action}\n{export}\n\nLatest durable summary:\n{summary}")
        } else {
            row["source_build"]["phase"].as_str().map_or_else(
                || {
                    "Enable and use this module to publish run-specific progress and actions."
                        .into()
                },
                |phase| {
                    format!(
                        "Source build: {phase} — {}",
                        row["source_build"]["detail"].as_str().unwrap_or("no detail")
                    )
                },
            )
        };
        frame.render_widget(
            Paragraph::new(crate::sanitize::sanitize_display_text(&details))
                .wrap(Wrap { trim: false }),
            regions[3],
        );
    }
    frame.render_widget(
        Paragraph::new(crate::sanitize::sanitize_display_text(&format!(
            "{} · Esc back",
            menu.status
        )))
        .wrap(Wrap { trim: false }),
        regions[4],
    );
}
