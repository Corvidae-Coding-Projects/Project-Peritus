//! Discoverable local search, attachment, theme, and shortcut overlays.

use crate::{
    highlight::Theme,
    model::{AppModel, chat::convenience::Panel},
    preferences::Shortcut,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let ui = &model.chat.convenience;
    let Some(panel) = ui.panel else { return };
    let title = match panel {
        Panel::Find => " Find in saved transcript ",
        Panel::Files => " Attach a project file ",
        Panel::Keys => " Keyboard shortcuts ",
        Panel::Theme => " Syntax theme ",
        Panel::Code => " Copy a code block ",
    };
    let sections =
        Layout::vertical([Constraint::Length(3), Constraint::Min(1), Constraint::Length(2)])
            .split(area);
    let query=match panel {
        Panel::Find|Panel::Files=>format!("> {}{}",crate::sanitize::sanitize_display_text(&ui.query),if ui.loading{"  · searching…"}else{""}),
        Panel::Keys if ui.capturing=>"Press the new shortcut now. Esc cancels. Reserved keys stay protected.".to_owned(),
        Panel::Keys=>"Select an action and press Enter to change its shortcut. /keys reset restores defaults.".to_owned(),
        Panel::Theme=>format!("Current: {} · arrows preview · Enter saves",ui.preferences.theme.name()),
        Panel::Code=>"Select a block and press Enter to copy its literal contents.".to_owned(),
    };
    frame.render_widget(
        Paragraph::new(query)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(title)),
        sections[0],
    );
    if let Some(preview) = &ui.preview {
        let lines = super::wrapped_lines(
            preview.lines().map(|l| Line::raw(crate::sanitize::sanitize_display_text(l))).collect(),
            usize::from(sections[1].width.saturating_sub(2)),
        );
        let max = lines.len().saturating_sub(usize::from(sections[1].height.saturating_sub(2)));
        frame.render_widget(
            Paragraph::new(lines.into_iter().skip(ui.scroll.min(max)).collect::<Vec<_>>())
                .block(Block::default().borders(Borders::ALL).title("Full result · c copies")),
            sections[1],
        );
    } else if panel == Panel::Theme {
        let parts =
            Layout::horizontal([Constraint::Length(18), Constraint::Min(1)]).split(sections[1]);
        list(frame, parts[0], vec!["dark".into(), "light".into(), "plain".into()], ui.selected);
        let theme = [Theme::Dark, Theme::Light, Theme::Plain][ui.selected.min(2)];
        let mut lines = crate::highlight::code(
            "// Syntax preview\nfn main() {\n    let message = \"Hello, Peritus\";\n    println!(\"{message}\");\n}",
            "rust",
            theme,
        );
        lines.push(Line::raw(""));
        lines.extend(crate::highlight::diff("@@ -1 +1 @@\n-old_value\n+new_value", theme));
        frame.render_widget(
            Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Preview")),
            parts[1],
        );
    } else {
        let items: Vec<String> = match panel {
            Panel::Find => ui.search.as_ref().map_or_else(Vec::new, |page| {
                page.hits
                    .iter()
                    .map(|h| format!("#{} · {} · {}", h.sequence, h.kind, h.title))
                    .collect()
            }),
            Panel::Files => ui
                .files
                .iter()
                .map(|p| {
                    ui.file_root
                        .as_ref()
                        .and_then(|root| p.strip_prefix(root).ok())
                        .unwrap_or(p)
                        .to_string_lossy()
                        .into_owned()
                })
                .collect(),
            Panel::Keys => Shortcut::ALL
                .iter()
                .map(|a| format!("{:<16} {}", a.name(), ui.preferences.binding(*a)))
                .collect(),
            Panel::Code => ui
                .blocks
                .iter()
                .map(|(language, body)| {
                    format!(
                        "{} · {} lines · {}",
                        if language.is_empty() { "code" } else { language },
                        body.lines().count(),
                        body.lines().next().unwrap_or_default()
                    )
                })
                .collect(),
            Panel::Theme => Vec::new(),
        };
        if items.is_empty() {
            let message = if ui.loading {
                "Searching locally…"
            } else if !ui.error.is_empty() {
                &ui.error
            } else if ui.query.is_empty() {
                "Type to search. The main message draft is retained."
            } else {
                "No matches."
            };
            frame.render_widget(
                Paragraph::new(crate::sanitize::sanitize_display_text(message))
                    .wrap(Wrap { trim: false }),
                sections[1],
            );
        } else {
            list(frame, sections[1], items, ui.selected);
        }
    }
    let footer = if !ui.error.is_empty() {
        ui.error.clone()
    } else if ui.preview.is_some() {
        "↑↓/PgUp/PgDn scroll · c copy · Esc returns to results".into()
    } else {
        match panel {
            Panel::Find => {
                let coverage = ui
                    .search
                    .as_ref()
                    .and_then(|p| p.first_sequence)
                    .filter(|s| *s > 1)
                    .map_or(String::new(), |s| {
                        format!(" · earlier uncaptured activity before #{s} unavailable")
                    });
                let more = if ui.search.as_ref().is_some_and(|p| p.truncated) {
                    " · partial results; narrow search"
                } else {
                    ""
                };
                format!(
                    "Captured local history; disconnected gaps may be absent · ↑↓ select · Enter opens · Esc back{more}{coverage}"
                )
            }
            Panel::Files => {
                "Up to 200 fuzzy matches · ↑↓ select · Enter opens attachment preview · Esc back"
                    .into()
            }
            Panel::Keys => "↑↓ select · Enter remaps · Esc back · changes saved locally".into(),
            Panel::Theme => "↑↓ preview · Enter saves · Esc back · dark/light/plain".into(),
            Panel::Code => "↑↓ select · Enter copies · Tab previews full block · Esc back".into(),
        }
    };
    frame.render_widget(
        Paragraph::new(crate::sanitize::sanitize_display_text(&footer)).wrap(Wrap { trim: false }),
        sections[2],
    );
}

fn list(frame: &mut Frame<'_>, area: Rect, items: Vec<String>, selected: usize) {
    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(selected.min(items.len() - 1)));
    }
    frame.render_stateful_widget(
        List::new(
            items
                .into_iter()
                .map(|s| ListItem::new(crate::sanitize::sanitize_display_text(&s)))
                .collect::<Vec<_>>(),
        )
        .highlight_symbol("▸ ")
        .highlight_style(
            Style::default().fg(super::ACCENT).add_modifier(Modifier::BOLD | Modifier::REVERSED),
        ),
        area,
        &mut state,
    );
}
