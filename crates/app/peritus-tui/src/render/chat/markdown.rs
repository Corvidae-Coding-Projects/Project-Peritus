//! CommonMark presentation stays terminal-inert and never fetches linked resources.

use crate::highlight::{self, Theme};
use crate::model::AppModel;
use peritus_app_protocol::ProductActivity;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

// This private style marker distinguishes code-block lines from inline code without parsing
// Markdown a second time. It never reaches the terminal: decorate_code removes it below.
const CODE_BLOCK: Color = Color::Rgb(1, 2, 3);

#[derive(Clone)]
struct Styles;

impl tui_markdown::StyleSheet for Styles {
    fn code(&self) -> Style {
        Style::default().underline_color(CODE_BLOCK)
    }
    fn heading(&self, level: u8) -> Style {
        let style = Style::default().fg(super::ACCENT).add_modifier(ratatui::style::Modifier::BOLD);
        if level == 1 { style.add_modifier(ratatui::style::Modifier::UNDERLINED) } else { style }
    }
    fn heading_marker(&self, _level: u8) -> &str {
        ""
    }
    fn code_block_fence(&self) -> &str {
        "──"
    }
    fn link(&self) -> Style {
        Style::default().fg(super::ACCENT).add_modifier(ratatui::style::Modifier::UNDERLINED)
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
    let theme = model.chat.convenience.preferences.theme;
    let lines = if theme == Theme::Dark {
        render(source, width)
    } else {
        render_with_theme(source, width, theme)
    };
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
    render_with_theme(source, width, Theme::Dark)
}

pub(super) fn render_with_theme(source: &str, width: usize, theme: Theme) -> Vec<Line<'static>> {
    let safe = crate::sanitize::sanitize_display_text(source);
    let options =
        tui_markdown::Options::new(Styles).table_width(u16::try_from(width).unwrap_or(u16::MAX));
    let mut lines: Vec<_> = tui_markdown::from_str_with_options(&safe, &options)
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
    decorate_code(&mut lines, theme);
    lines
}

fn decorate_code(lines: &mut [Line<'static>], theme: Theme) {
    let mut index = 0;
    while index < lines.len() {
        if !marked_code(&lines[index])
            || !lines[index].spans.last().is_some_and(|span| span.content.starts_with("──"))
        {
            index += 1;
            continue;
        }
        let end = lines[index..]
            .iter()
            .position(|line| !marked_code(line))
            .map_or(lines.len(), |length| index + length);
        if end.saturating_sub(index) >= 2 {
            let language = lines[index]
                .spans
                .last()
                .and_then(|span| span.content.strip_prefix("──"))
                .unwrap_or_default()
                .to_owned();
            let body = &mut lines[index + 1..end - 1];
            // The Markdown renderer emits one literal final span per code line; preceding spans
            // are list/blockquote prefixes. Preserve those exactly while replacing only styling.
            let source = body
                .iter()
                .map(|line| line.spans.last().map_or("", |span| span.content.as_ref()))
                .collect::<Vec<_>>()
                .join("\n");
            for (line, highlighted) in
                body.iter_mut().zip(highlight::code(&source, &language, theme))
            {
                if !line.spans.is_empty() {
                    line.spans.pop();
                    line.spans.extend(highlighted.spans);
                }
            }
        }
        index = end;
    }
    for line in lines {
        if line.style.underline_color == Some(CODE_BLOCK) {
            line.style.underline_color = None;
        }
        for span in &mut line.spans {
            if span.style.underline_color == Some(CODE_BLOCK) {
                span.style.underline_color = None;
            }
        }
    }
}

fn marked_code(line: &Line<'_>) -> bool {
    line.style.underline_color == Some(CODE_BLOCK)
        || line.spans.last().is_some_and(|span| span.style.underline_color == Some(CODE_BLOCK))
}
