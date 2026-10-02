//! Keyboard and deliberate-paste ownership for the persistent composer.

use super::{AppModel, Effect, NoticeLevel};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use peritus_app_protocol::ProductRunControlAction;

impl AppModel {
    pub(in crate::model) fn paste_chat(&mut self, text: &str) {
        self.chat.interrupt_requested = false;
        self.chat.mouse_anchor = None;
        let text: String =
            text.chars().filter(|ch| !ch.is_control() || *ch == '\n' || *ch == '\t').collect();
        let selected = self.chat.selection().unwrap_or(self.chat.cursor..self.chat.cursor);
        if self.chat.buffer.len().saturating_sub(selected.len()).saturating_add(text.len())
            > peritus_app_protocol::MAX_PRODUCT_TASK_BYTES
        {
            self.notice(NoticeLevel::Warning, "Message is too large; paste a smaller selection");
            return;
        }
        self.chat.cursor = selected.start + text.len();
        self.chat.buffer.replace_range(selected, &text);
        self.chat.selection_anchor = None;
    }

    pub(in crate::model) fn paste_chat_event(&mut self, text: &str) {
        self.chat.output_selection = None;
        self.chat.output_mode.resume_for_paste();
        if self.paste_file_field(text) || self.paste_image_field(text) {
            return;
        }
        if self.chat.workbench.open
            || self.chat.doctor.is_some()
            || self.chat.model_picker()
            || self.chat.effort_picker()
        {
            return;
        }
        let previous = self.chat.buffer.clone();
        let cursor = self.chat.selection().map_or(self.chat.cursor, |range| range.start);
        self.paste_chat(text);
        // A paste that introduces or modifies the command token cannot become an intent.
        // Pasting arguments after a fully keyboard-entered token remains deliberate input.
        let token_end = previous.find(char::is_whitespace).unwrap_or(previous.len());
        if previous != self.chat.buffer
            && self.chat.buffer.trim_start().starts_with('/')
            && cursor <= token_end
        {
            self.chat.pasted_command = true;
        }
    }
    fn output_selection_key(&mut self, key: KeyEvent) -> Option<Vec<Effect>> {
        self.chat.output_selection.as_ref()?;
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('c')
            && self
                .chat
                .output_selection
                .as_ref()
                .is_some_and(|selection| selection.selected_text().is_some())
        {
            // Repeated copy keys while the helper is pending still belong to this selection.
            return Some(self.copy_selected_output());
        }
        self.chat.output_selection = None;
        (key.code == KeyCode::Esc).then(Vec::new)
    }

    pub(in crate::model) fn handle_chat_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if let Some(effects) = self.output_selection_key(key) {
            return effects;
        }
        if self.chat.output_mode.handle_key(key) {
            return Vec::new();
        }
        self.chat.mouse_anchor = None;
        if !(key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c')) {
            self.chat.interrupt_requested = false;
        }
        if let Some(effects) = self.chat_control_key(key) {
            return effects;
        }
        if self.chat.doctor.is_some() {
            return self.doctor_key(key);
        }
        if self.chat.workbench.open {
            return self.workbench_key(key);
        }
        if self.chat.effort_picker() {
            return self.effort_picker_key(key);
        }
        if self.chat.model_picker() {
            return self.model_picker_key(key);
        }
        let commands = self.chat.matching_commands();
        match key.code {
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => self.paste_chat("\n"),
            KeyCode::Enter => return self.submit_chat(),
            KeyCode::Tab if !commands.is_empty() => {
                self.chat.buffer =
                    format!("{} ", commands[self.chat.command_selection.min(commands.len() - 1)].0);
                self.chat.cursor = self.chat.buffer.len();
                self.chat.selection_anchor = None;
                self.chat.command_selection = 0;
                // Explicit selection from the local catalog creates a keyboard-owned intent.
                self.chat.pasted_command = false;
            }
            KeyCode::Up if !commands.is_empty() => {
                self.chat.command_selection = self.chat.command_selection.saturating_sub(1);
            }
            KeyCode::Down if !commands.is_empty() => {
                self.chat.command_selection =
                    (self.chat.command_selection + 1).min(commands.len() - 1);
            }
            KeyCode::Up | KeyCode::Down => {
                self.move_chat_cursor_vertically(key);
            }
            KeyCode::PageUp => self.chat.scroll = self.chat.scroll.saturating_add(12),
            KeyCode::PageDown => self.chat.scroll = self.chat.scroll.saturating_sub(12),
            KeyCode::End if self.chat.buffer.is_empty() => self.chat.scroll = 0,
            KeyCode::Esc if self.chat.selection_anchor.take().is_some() => {}
            KeyCode::Esc => {
                self.chat.workbench.snapshot_refresh_command = None;
                self.chat.expanded = false;
                self.chat.command_selection = 0;
            }
            _ => {
                let selected_bytes = self.chat.selection().map_or(0, |range| range.len());
                let inserted_bytes = match key.code {
                    KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        ch.len_utf8()
                    }
                    _ => 0,
                };
                if inserted_bytes == 0
                    || self.chat.buffer.len() - selected_bytes + inserted_bytes
                        <= peritus_app_protocol::MAX_PRODUCT_TASK_BYTES
                {
                    let _ = crate::input::selection::edit(
                        &mut self.chat.buffer,
                        &mut self.chat.cursor,
                        &mut self.chat.selection_anchor,
                        key,
                    );
                    if self.chat.buffer.is_empty() {
                        self.chat.pasted_command = false;
                    }
                }
            }
        }
        Vec::new()
    }

    fn chat_control_key(&mut self, key: KeyEvent) -> Option<Vec<Effect>> {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('r') => return Some(self.request_reconnect()),
                KeyCode::Char('q') => {
                    self.quitting = true;
                    return Some(vec![Effect::Quit]);
                }
                KeyCode::Char('c') => {
                    if self.chat_work_active() && !self.chat.interrupt_requested {
                        self.chat.interrupt_requested = true;
                        let effects = self.chat_control(ProductRunControlAction::Cancel);
                        self.notice(NoticeLevel::Info, if effects.is_empty() {
                            "Stop could not be sent. Press Ctrl+C again to close; daemon work may continue."
                        } else {
                            "Stop requested. Press Ctrl+C again to close without waiting."
                        });
                        return Some(effects);
                    }
                    self.quitting = true;
                    return Some(vec![Effect::Quit]);
                }
                _ => {}
            }
        }
        None
    }

    fn move_chat_cursor_vertically(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::SHIFT) {
            self.chat.selection_anchor.get_or_insert(self.chat.cursor);
        } else {
            self.chat.selection_anchor = None;
        }
        let width = self.chat.viewport.map_or(80, |area| area.width.saturating_sub(2));
        self.chat.cursor = crate::input::composer::vertical_cursor(
            &self.chat.buffer,
            self.chat.cursor,
            key.code == KeyCode::Down,
            usize::from(width),
        );
    }
}
