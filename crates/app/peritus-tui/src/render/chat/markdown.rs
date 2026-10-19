//! CommonMark presentation stays terminal-inert and never fetches linked resources.

use crate::model::AppModel;
use peritus_app_protocol::ProductActivity;
use ratatui::text::{Line, Span};

#[derive(Clone)]
struct Styles;

impl tui_markdown::StyleSheet for Styles {
    fn heading(&self, level: u8) -> ratatui::style::Style {
        let style = ratatui::style::Style::default()
            .fg(super::ACCENT)
            .add_modifier(ratatui::style::Modifier::BOLD);
        if level == 1 { style.add_modifier(ratatui::style::Modifier::UNDERLINED) } else { style }
    }
    fn heading_marker(&self, _level: u8) -> &str {
        ""
    }
    fn code_block_fence(&self) -> &str {
        "──"
    }
    fn link(&self) -> ratatui::style::Style {
        ratatui::style::Style::default()
            .fg(super::ACCENT)
            .add_modifier(ratatui::style::Modifier::UNDERLINED)
    }
}

pub(super) fn lines(
    model: &AppModel,
    activity: &ProductActivity,
    width: usize,
) -> Vec<Line<'static>> {
    cached(model, activity.sequence(), activity.text(), width)
}

pub(super) fn cached(
    model: &AppModel,
    sequence: u64,
    source: &str,
    width: usize,
) -> Vec<Line<'static>> {
    let mut cache = model.chat.formatted_messages.borrow_mut();
    if let Some(cached) = cache.get(&sequence)
        && cached.width == width
        && cached.source == source
    {
        return cached.lines.clone();
    }
    let lines = render(source, width);
    cache.insert(
        sequence,
        crate::model::chat::FormattedMessage {
            source: source.to_owned(),
            width,
            lines: lines.clone(),
        },
    );
    lines
}

pub(super) fn render(source: &str, width: usize) -> Vec<Line<'static>> {
    let safe = crate::sanitize::sanitize_display_text(source);
    let options =
        tui_markdown::Options::new(Styles).table_width(u16::try_from(width).unwrap_or(u16::MAX));
    let lines: Vec<_> = tui_markdown::from_str_with_options(&safe, &options)
        .lines
        .into_iter()
        .map(|line| {
            Line::from(
                line.spans
                    .into_iter()
                    // Entity decoding can introduce controls after the first sanitization.
                    .map(|span| {
                        Span::styled(
                            crate::sanitize::sanitize_display_text(&span.content),
                            span.style,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
            .style(line.style)
        })
        .collect();
    lines
}
