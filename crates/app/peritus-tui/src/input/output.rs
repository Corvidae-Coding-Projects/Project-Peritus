//! Selection of a frozen visible transcript, preserving UTF-8 and hard line breaks.

use ratatui::{
    layout::{Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
};
use std::ops::Range;

#[derive(Clone, Debug)]
pub struct OutputRow {
    pub line: Line<'static>,
    pub hard_break: bool,
}

#[derive(Debug)]
pub struct OutputSelection {
    pub area: Rect,
    pub menu: Option<Rect>,
    pub dragging: bool,
    pub copy_operation: Option<peritus_app_protocol::ControlOperationId>,
    rows: Vec<OutputRow>,
    text: String,
    offsets: Vec<usize>,
    anchor: usize,
    cursor: usize,
}

impl OutputSelection {
    pub fn new(area: Rect, rows: Vec<OutputRow>, position: Position) -> Self {
        let mut text = String::new();
        let mut offsets = Vec::with_capacity(rows.len());
        for (index, row) in rows.iter().enumerate() {
            offsets.push(text.len());
            text.push_str(&row.line.to_string());
            if row.hard_break && index + 1 < rows.len() {
                text.push('\n');
            }
        }
        let mut selection = Self {
            area,
            menu: None,
            dragging: true,
            copy_operation: None,
            rows,
            text,
            offsets,
            anchor: 0,
            cursor: 0,
        };
        selection.cursor = selection.hit(position);
        selection.anchor = selection.cursor;
        selection
    }

    pub fn extend(&mut self, position: Position) {
        self.cursor = self.hit(position);
        self.menu = None;
    }

    fn hit(&self, position: Position) -> usize {
        let index = usize::from(position.y.saturating_sub(self.area.y))
            .min(self.rows.len().saturating_sub(1));
        let Some(row) = self.rows.get(index) else { return 0 };
        let text = row.line.to_string();
        let span = Span::raw(&text);
        let target = usize::from(position.x.saturating_sub(self.area.x));
        let (mut cell, mut byte) = (0_usize, 0_usize);
        let mut nearest = (target, 0);
        // Use the same indivisible graphemes as ratatui; combining marks and emoji stay intact.
        for grapheme in span.styled_graphemes(Style::default()) {
            cell += Span::raw(grapheme.symbol).width();
            byte += grapheme.symbol.len();
            let distance = cell.abs_diff(target);
            if distance < nearest.0 {
                nearest = (distance, byte);
            }
        }
        self.offsets[index] + nearest.1
    }

    fn range(&self) -> Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }

    pub fn selected_text(&self) -> Option<&str> {
        let range = self.range();
        (!range.is_empty()).then(|| &self.text[range])
    }

    pub fn open_menu(&mut self, viewport: Rect, position: Position) {
        self.dragging = false;
        if self.selected_text().is_some() && viewport.width >= 8 && viewport.height >= 3 {
            self.menu = Some(Rect::new(
                position.x.clamp(viewport.x, viewport.right() - 8),
                position.y.clamp(viewport.y, viewport.bottom() - 3),
                8,
                3,
            ));
        }
    }

    pub fn copy_hit(&self, position: Position) -> bool {
        self.menu.is_some_and(|area| {
            position.y == area.y + 1 && position.x > area.x && position.x < area.right() - 1
        })
    }

    pub fn highlighted_lines(&self) -> Vec<Line<'static>> {
        let range = self.range();
        self.rows
            .iter()
            .zip(&self.offsets)
            .map(|(row, &offset)| {
                let mut line = Line::default().style(row.line.style);
                let mut byte = offset;
                for span in &row.line.spans {
                    let end = byte + span.content.len();
                    let from = range.start.clamp(byte, end) - byte;
                    let to = range.end.clamp(byte, end) - byte;
                    for (part, selected) in [
                        (&span.content[..from], false),
                        (&span.content[from..to], true),
                        (&span.content[to..], false),
                    ] {
                        if !part.is_empty() {
                            let style = if selected {
                                span.style.add_modifier(Modifier::REVERSED)
                            } else {
                                span.style
                            };
                            line.spans.push(Span::styled(part.to_owned(), style));
                        }
                    }
                    byte = end;
                }
                line
            })
            .collect()
    }
}
