//! Keep the active panel visible when the tab strip is wider than the terminal.

use super::{ACCENT, MUTED};
use crate::model::{AppModel, ConnectionStatus, View};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Tabs},
};

pub(super) fn render_tabs(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let mut titles = View::ALL
        .iter()
        .enumerate()
        .map(|(index, view)| Line::from(format!(" {}:{} ", index + 1, view.label())))
        .collect::<Vec<_>>();
    let selected = View::ALL.iter().position(|view| *view == model.view).unwrap_or(0);
    let available = usize::from(area.width.saturating_sub(6));
    let mut start = 0;
    let mut end = selected + 1;
    let mut width = titles[..end].iter().map(Line::width).sum::<usize>() + end - 1;
    while width > available && start < selected {
        width -= titles[start].width() + 1;
        start += 1;
    }
    while end < titles.len() && width + 1 + titles[end].width() <= available {
        width += 1 + titles[end].width();
        end += 1;
    }
    if start > 0 {
        titles[start].spans.insert(0, "‹ ".into());
    }
    if end < titles.len() {
        titles[end - 1].spans.push(" ›".into());
    }
    let title = match &model.connection {
        ConnectionStatus::Connecting => " Peritus · connecting ".to_owned(),
        ConnectionStatus::Online { server, downgraded } => {
            let suffix = if *downgraded { " · negotiated downgrade" } else { "" };
            format!(" Peritus · {server}{suffix} ")
        }
        ConnectionStatus::Disconnected(_) => " Peritus · disconnected ".to_owned(),
    };
    let tabs = Tabs::new(titles.drain(start..end))
        .block(Block::default().borders(Borders::ALL).title(title))
        .padding("", "")
        .divider("│")
        .select(selected - start)
        .style(Style::default().fg(MUTED))
        .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn every_selected_panel_has_a_visible_highlighted_tab_on_small_terminals() {
        for width in [48, 80, 132] {
            for (index, view) in View::ALL.into_iter().enumerate() {
                let mut model = AppModel::new([64; 32]);
                model.view = view;
                let mut terminal = Terminal::new(TestBackend::new(width, 3)).unwrap();
                terminal.draw(|frame| render_tabs(frame, frame.area(), &model)).unwrap();
                let cells = terminal.backend().buffer().content();
                let text = cells.iter().map(ratatui::buffer::Cell::symbol).collect::<String>();
                let label = format!("{}:{}", index + 1, view.label());
                assert!(text.contains(&label), "missing {label} at width {width}: {text}");
                assert!(
                    cells
                        .iter()
                        .any(|cell| cell.fg == ACCENT && cell.modifier.contains(Modifier::BOLD))
                );
            }
        }
    }
}
