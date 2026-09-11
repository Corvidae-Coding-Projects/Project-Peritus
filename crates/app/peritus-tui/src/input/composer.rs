//! One character-cell layout for composer rendering, cursor placement, and mouse hit testing.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
};
use std::ops::Range;

pub struct DraftLayout {
    pub lines: Vec<Line<'static>>,
    pub row: usize,
    pub column: usize,
    stops: Vec<Vec<(usize, usize)>>,
}

impl DraftLayout {
    pub fn offset(&self, area: Rect) -> usize {
        self.row.saturating_sub(usize::from(area.height.saturating_sub(2)).max(1) - 1)
    }

    pub fn hit(&self, row: usize, column: usize) -> usize {
        let stops = &self.stops[row.min(self.stops.len() - 1)];
        // Choose the nearest character boundary; ties place before the character.
        stops.iter().min_by_key(|(cell, _)| cell.abs_diff(column)).map_or(0, |(_, byte)| *byte)
    }
}

pub fn regions(area: Rect, lines: usize, working: bool) -> [Rect; 5] {
    Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(u16::from(working)),
        Constraint::Length(u16::try_from(lines.clamp(1, 6)).unwrap_or(6) + 2),
        Constraint::Length(2),
    ])
    .areas(area)
}

pub fn layout(
    text: &str,
    cursor: usize,
    selection: Option<&Range<usize>>,
    width: usize,
) -> DraftLayout {
    let width = width.max(1);
    let mut lines = vec![Line::default()];
    let mut stops = vec![vec![(0, 0)]];
    let mut column = 0;
    let mut position = (0, 0);
    for (index, character) in text.char_indices() {
        let shown = if character == '\t' { "    ".to_owned() } else { character.to_string() };
        let cells = Span::raw(shown.as_str()).width().min(width);
        if character != '\n' && column + cells > width {
            lines.push(Line::default());
            stops.push(vec![(0, index)]);
            column = 0;
        }
        if index == cursor {
            position = (lines.len() - 1, column);
        }
        let selected = selection.as_ref().is_some_and(|range| range.contains(&index));
        let style = if selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        if character == '\n' {
            if selected && column < width {
                append(&mut lines, " ", style);
            }
            lines.push(Line::default());
            stops.push(vec![(0, index + 1)]);
            column = 0;
        } else {
            append(&mut lines, &shown, style);
            column += cells;
            if let Some(row) = stops.last_mut() {
                row.push((column, index + character.len_utf8()));
            }
        }
    }
    if column == width {
        lines.push(Line::default());
        stops.push(vec![(0, text.len())]);
        column = 0;
    }
    if cursor == text.len() {
        position = (lines.len() - 1, column);
    }
    DraftLayout { lines, row: position.0, column: position.1, stops }
}

fn append(lines: &mut [Line<'static>], text: &str, style: Style) {
    if let Some(line) = lines.last_mut() {
        if let Some(span) = line.spans.last_mut()
            && span.style == style
        {
            span.content.to_mut().push_str(text);
        } else {
            line.spans.push(Span::styled(text.to_owned(), style));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_and_clicks_track_wraps_wide_text_and_newlines() {
        let text = "ab界cd\nλ\n";
        let draft = layout(text, text.len(), None, 4);
        assert_eq!(
            draft.lines.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["ab界", "cd", "λ", ""]
        );
        assert_eq!((draft.row, draft.column), (3, 0));
        assert_eq!(draft.hit(0, 2), 2);
        assert_eq!(draft.hit(0, 3), 2);
        assert_eq!(draft.hit(1, 0), "ab界".len());
        assert_eq!(draft.hit(1, 3), "ab界cd".len());
        assert_eq!(draft.hit(2, 1), "ab界cd\nλ".len());
        assert_eq!(draft.hit(20, 20), text.len());
        let draft = layout(text, "ab界".len(), None, 4);
        assert_eq!((draft.row, draft.column), (1, 0));
    }

    #[test]
    fn exact_width_cursor_has_a_visible_next_line() {
        let draft = layout("abcd", 4, None, 4);
        assert_eq!(draft.lines.len(), 2);
        assert_eq!((draft.row, draft.column), (1, 0));
        assert_eq!(draft.hit(1, 0), 4);
    }

    #[test]
    fn tabs_selection_and_combining_text_keep_byte_boundaries() {
        let draft = layout("a\t界e\u{301}", 2, Some(&(1..5)), 6);
        assert_eq!(draft.hit(0, 4), 2);
        assert_eq!(draft.hit(1, 0), 2);
        assert_eq!((draft.row, draft.column), (1, 0));
        assert!(draft.lines[0].spans[1].style.add_modifier.contains(Modifier::REVERSED));
        assert_eq!(draft.lines[1].to_string(), "界e\u{301}");
    }
}
