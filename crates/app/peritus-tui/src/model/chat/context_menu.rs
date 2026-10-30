//! Mouse paste stays in the live composer; clipboard reads never submit its contents.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Margin, Position, Rect};

use super::{AppModel, Effect, NoticeLevel};
use crate::{
    input::composer,
    local::{ClipboardSource, LocalOperation},
};

impl AppModel {
    pub(super) fn handle_composer_menu(&mut self, mouse: MouseEvent) -> Option<Vec<Effect>> {
        if !self.composer_available() || self.chat.selecting_output() {
            self.chat.composer_menu = None;
            return None;
        }
        let position = Position::new(mouse.column, mouse.row);
        if let Some(menu) = self.chat.composer_menu {
            if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                self.chat.composer_menu = None;
                if menu.inner(Margin::new(1, 1)).contains(position) {
                    return Some(if position.y == menu.y + 1 {
                        self.copy_composer_selection()
                    } else {
                        self.paste_from_clipboard(ClipboardSource::Clipboard)
                    });
                }
            } else if matches!(mouse.kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown) {
                self.chat.composer_menu = None;
            }
        }
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Right | MouseButton::Middle)) {
            return None;
        }
        let viewport = self.chat.viewport?;
        let draft = composer::layout(
            &self.chat.buffer,
            self.chat.cursor,
            None,
            usize::from(viewport.width.saturating_sub(2)),
        );
        let area = composer::regions(
            viewport,
            draft.lines.len(),
            self.chat.working.elapsed_seconds().is_some(),
        )[3];
        if !area.inner(Margin::new(1, 1)).contains(position) {
            self.chat.composer_menu = None;
            return None;
        }
        self.chat.mouse_anchor = None;
        self.chat.output_selection = None;
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Middle)) {
            return Some(self.paste_from_clipboard(ClipboardSource::Primary));
        }
        if viewport.width >= 9 && viewport.height >= 4 {
            self.chat.composer_menu = Some(Rect::new(
                position.x.clamp(viewport.x, viewport.right() - 9),
                position.y.clamp(viewport.y, viewport.bottom() - 4),
                9,
                4,
            ));
        }
        Some(Vec::new())
    }

    fn copy_composer_selection(&mut self) -> Vec<Effect> {
        let Some(range) = self.chat.selection() else { return Vec::new() };
        let text = self.chat.buffer[range].to_owned();
        let Ok(operation) =
            peritus_app_protocol::ControlOperationId::new(self.ids.bytes(b"composer-copy"))
        else {
            return Vec::new();
        };
        vec![Effect::CopyText { operation, text }]
    }

    fn paste_from_clipboard(&mut self, source: ClipboardSource) -> Vec<Effect> {
        self.chat.composer_menu = None;
        let effects = self.start_local(LocalOperation::ReadClipboardText(source));
        self.notice(
            NoticeLevel::Info,
            if effects.is_empty() {
                "Another local action is still finishing; draft retained."
            } else {
                "Reading clipboard text…"
            },
        );
        effects
    }
}
