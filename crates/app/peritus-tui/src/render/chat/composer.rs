//! Shared character-cell wrapping for composer content and its cursor.

use ratatui::text::Span;

pub(super) struct DraftLayout {
    pub lines: Vec<String>,
    pub row: usize,
    pub column: usize,
}

pub(super) fn layout(text: &str, cursor: usize, width: usize) -> DraftLayout {
    let width = width.max(1);
    let mut lines = vec![String::new()];
    let mut column = 0;
    let mut position = (0, 0);
    for (index, character) in text.char_indices() {
        let shown = if character == '\t' { "    ".to_owned() } else { character.to_string() };
        let cells = Span::raw(shown.as_str()).width().min(width);
        if character != '\n' && column + cells > width {
            lines.push(String::new());
            column = 0;
        }
        if index == cursor {
            position = (lines.len() - 1, column);
        }
        if character == '\n' {
            lines.push(String::new());
            column = 0;
        } else {
            if let Some(line) = lines.last_mut() {
                line.push_str(&shown);
            }
            column += cells;
        }
    }
    if cursor == text.len() {
        if column == width {
            lines.push(String::new());
            column = 0;
        }
        position = (lines.len() - 1, column);
    }
    DraftLayout { lines, row: position.0, column: position.1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_tracks_cell_wraps_wide_text_and_trailing_newlines() {
        let text = "ab界cd\nλ\n";
        let draft = layout(text, text.len(), 4);
        assert_eq!(draft.lines, ["ab界", "cd", "λ", ""]);
        assert_eq!((draft.row, draft.column), (3, 0));
        let draft = layout(text, "ab界".len(), 4);
        assert_eq!((draft.row, draft.column), (1, 0));
    }

    #[test]
    fn exact_width_cursor_has_a_visible_next_line() {
        let draft = layout("abcd", 4, 4);
        assert_eq!(draft.lines, ["abcd", ""]);
        assert_eq!((draft.row, draft.column), (1, 0));
    }
}
