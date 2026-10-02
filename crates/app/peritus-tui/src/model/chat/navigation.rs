//! Composer selection and mouse ownership, using the same geometry as rendering.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;

use super::{AppModel, ChatUi, Effect, NoticeLevel};
use crate::{input::composer, model::View};

const MOUSE_WHEEL_ROWS: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputMode {
    Live,
    Selecting,
}

impl OutputMode {
    pub(super) const fn begin_terminal_selection(&mut self) {
        *self = Self::Selecting;
    }

    pub(super) fn resume_for_paste(&mut self) {
        if *self == Self::Selecting {
            *self = Self::Live;
        }
    }

    pub(super) fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::KeyCode;
        match *self {
            Self::Selecting => {
                if matches!(key.code, KeyCode::Esc | KeyCode::F(2)) {
                    *self = Self::Live;
                }
                // The terminal owns copy keys; they must never cancel daemon work.
                true
            }
            Self::Live if key.code == KeyCode::F(2) => {
                *self = Self::Selecting;
                true
            }
            Self::Live => false,
        }
    }
}

impl ChatUi {
    pub(crate) fn selection(&self) -> Option<std::ops::Range<usize>> {
        crate::input::selection::range(self.selection_anchor, self.cursor)
    }
}

impl AppModel {
    pub(in crate::model) fn handle_chat_mouse(&mut self, mouse: MouseEvent) -> Vec<Effect> {
        if let Some(effects) = self.handle_output_mouse(mouse) {
            return effects;
        }
        self.handle_composer_mouse(mouse);
        Vec::new()
    }

    pub(in crate::model) fn output_available(&self) -> bool {
        self.view == View::Conversation
            && self.editor.is_none()
            && !self.chat.selecting_output()
            && self.chat.doctor.is_none()
            && !self.chat.workbench.open
            && !self.chat.effort_picker()
            && !self.chat.model_picker()
            && self.chat.matching_commands().is_empty()
    }

    pub(in crate::model) fn copy_selected_output(&mut self) -> Vec<Effect> {
        let Some(selection) = &mut self.chat.output_selection else { return Vec::new() };
        if selection.copy_operation.is_some() {
            return Vec::new();
        }
        let Some(text) = selection.selected_text().map(str::to_owned) else { return Vec::new() };
        let Ok(operation) =
            peritus_app_protocol::ControlOperationId::new(self.ids.bytes(b"clipboard-copy"))
        else {
            return Vec::new();
        };
        selection.copy_operation = Some(operation);
        vec![Effect::CopyText { operation, text }]
    }

    pub(in crate::model) fn clipboard_written(
        &mut self,
        operation: peritus_app_protocol::ControlOperationId,
        result: Result<crate::action::ClipboardDestination, String>,
    ) -> Vec<Effect> {
        let owns_selection = self.chat.output_selection.as_mut().is_some_and(|selection| {
            if selection.copy_operation != Some(operation) {
                return false;
            }
            selection.copy_operation = None;
            true
        });
        match result {
        Ok(destination) => {
            if owns_selection { self.chat.output_selection = None; }
            self.notice(NoticeLevel::Info, match destination {
                crate::action::ClipboardDestination::Desktop => "Selected text copied to the desktop clipboard",
                crate::action::ClipboardDestination::Terminal => "Selected text sent to the terminal clipboard",
            });
        }
        Err(error) => self.notice(NoticeLevel::Error, format!("Copy failed: {error}. Selection retained; retry Copy or use F2 terminal selection.")),
    }
        Vec::new()
    }

    fn handle_output_mouse(&mut self, mouse: MouseEvent) -> Option<Vec<Effect>> {
        if !self.output_available() {
            return None;
        }
        let position = Position::new(mouse.column, mouse.row);
        let viewport = self.chat.viewport?;
        let area = crate::render::transcript_area(self, viewport);
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Right) => {
                if let Some(selection) = &mut self.chat.output_selection
                    && selection.selected_text().is_some()
                {
                    selection.open_menu(viewport, position);
                    return Some(Vec::new());
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if self
                    .chat
                    .output_selection
                    .as_ref()
                    .is_some_and(|selection| selection.copy_hit(position))
                {
                    return Some(self.copy_selected_output());
                }
                if area.contains(position) {
                    self.chat.mouse_anchor = None;
                    if mouse.modifiers.contains(KeyModifiers::SHIFT)
                        && let Some(selection) = &mut self.chat.output_selection
                    {
                        selection.extend(position);
                        selection.dragging = true;
                    } else {
                        self.chat.output_selection =
                            Some(crate::input::output::OutputSelection::new(
                                area,
                                crate::render::transcript_rows(self, area),
                                position,
                            ));
                    }
                    return Some(Vec::new());
                }
                self.chat.output_selection = None;
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(selection) = &mut self.chat.output_selection
                    && selection.dragging
                {
                    selection.extend(position);
                    return Some(Vec::new());
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some(selection) = &mut self.chat.output_selection
                    && selection.dragging
                {
                    selection.extend(position);
                    selection.dragging = false;
                    return Some(Vec::new());
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                self.chat.output_selection = None;
            }
            _ => {}
        }
        None
    }

    fn handle_composer_mouse(&mut self, mouse: MouseEvent) {
        if self.view == View::Conversation
            && self.editor.is_none()
            && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Right))
        {
            self.chat.mouse_anchor = None;
            self.chat.output_mode.begin_terminal_selection();
            return;
        }
        if matches!(mouse.kind, MouseEventKind::Up(MouseButton::Left)) {
            self.chat.mouse_anchor = None;
            return;
        }
        if self.view != View::Conversation
            || self.chat.selecting_output()
            || self.editor.is_some()
            || self.chat.doctor.is_some()
            || self.chat.workbench.open
            || self.chat.effort_picker()
            || self.chat.model_picker()
        {
            self.chat.mouse_anchor = None;
            return;
        }
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.chat.mouse_anchor = None;
                self.chat.scroll = self.chat.scroll.saturating_add(MOUSE_WHEEL_ROWS);
                return;
            }
            MouseEventKind::ScrollDown => {
                self.chat.mouse_anchor = None;
                self.chat.scroll = self.chat.scroll.saturating_sub(MOUSE_WHEEL_ROWS);
                return;
            }
            _ => {}
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
