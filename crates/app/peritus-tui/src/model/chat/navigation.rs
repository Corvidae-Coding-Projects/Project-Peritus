//! Composer selection and mouse ownership, using the same geometry as rendering.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;

use super::{AppModel, ChatUi};
use crate::{input::composer, model::View};

impl ChatUi {
    pub(crate) fn selection(&self) -> Option<std::ops::Range<usize>> {
        crate::input::selection::range(self.selection_anchor, self.cursor)
    }
}

impl AppModel {
    pub(in crate::model) fn handle_chat_mouse(&mut self, mouse: MouseEvent) {
        if matches!(mouse.kind, MouseEventKind::Up(MouseButton::Left)) {
            self.chat.mouse_anchor = None;
            return;
        }
        if self.view != View::Conversation
            || self.editor.is_some()
            || self.chat.doctor.is_some()
            || self.chat.workbench.open
            || self.chat.effort_picker()
            || self.chat.model_picker()
        {
            self.chat.mouse_anchor = None;
            return;
        }
        let extending = match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.chat.mouse_anchor = None;
                mouse.modifiers.contains(KeyModifiers::SHIFT)
            }
            MouseEventKind::Drag(MouseButton::Left) if self.chat.mouse_anchor.is_some() => true,
            _ => return,
        };
        let Some(viewport) = self.chat.viewport else { return };
        let draft = composer::layout(
            &self.chat.buffer,
            self.chat.cursor,
            self.chat.selection().as_ref(),
            usize::from(viewport.width.saturating_sub(2)),
        );
        let area = composer::regions(
            viewport,
            draft.lines.len(),
            self.chat.working.elapsed_seconds().is_some(),
        )[3];
        let inner = area.inner(ratatui::layout::Margin::new(1, 1));
        if !inner.contains(Position::new(mouse.column, mouse.row)) {
            return;
        }
        let cursor = draft.hit(
            draft.offset(area) + usize::from(mouse.row - inner.y),
            usize::from(mouse.column - inner.x),
        );
        if extending {
            self.chat.selection_anchor = Some(
                self.chat.mouse_anchor.or(self.chat.selection_anchor).unwrap_or(self.chat.cursor),
            );
        } else {
            self.chat.selection_anchor = None;
        }
        self.chat.cursor = cursor;
        // A plain click clears selection but retains its press position for dragging.
        self.chat.mouse_anchor = Some(self.chat.selection_anchor.unwrap_or(cursor));
    }
}
