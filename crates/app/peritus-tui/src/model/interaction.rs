//! Keyboard interaction and request construction for mutable UI workflows.

use super::*;
mod prompts;
mod terminal;

impl AppModel {
    pub(super) fn handle_terminal_event(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Paste(text) => {
                if self.editor.is_some() {
                    self.paste_editor_event(&text);
                    Vec::new()
                } else if self.view == View::Conversation {
                    self.paste_chat_event(&text);
                    Vec::new()
                } else if self.view == View::Terminal
                    && let Some(terminal) =
                        self.terminal.as_mut().filter(|terminal| terminal.capture_input())
                {
                    let bytes = terminal.paste_bytes(&text);
                    self.send_terminal_input(bytes)
                } else {
                    Vec::new()
                }
            }
            Event::Resize(columns, rows) => {
                self.chat.viewport = None;
                self.chat.output_selection = None;
                self.send_terminal_resize(columns, rows)
            }
            Event::Mouse(mouse) => self.handle_chat_mouse(mouse),
            Event::FocusGained | Event::FocusLost => Vec::new(),
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if !is_active_key(key) {
            return Vec::new();
        }
        let terminal_captures_input = self.view == View::Terminal
            && self.editor.is_none()
            && self.terminal.as_ref().is_some_and(TerminalSession::capture_input);
        if !terminal_captures_input
            && key.modifiers.contains(KeyModifiers::ALT)
            && matches!(
                key.code,
                KeyCode::PageUp | KeyCode::PageDown | KeyCode::Home | KeyCode::End
            )
        {
            let (maximum, page) = crate::render::status_scroll_metrics(self);
            let current = self.status_scroll.min(maximum);
            self.status_scroll = match key.code {
                KeyCode::PageUp => current.saturating_sub(page),
                KeyCode::PageDown => current.saturating_add(page).min(maximum),
                KeyCode::End => maximum,
                _ => 0,
            };
            return Vec::new();
        }
        if self.view == View::Conversation && self.editor.is_none() {
            return self.handle_chat_key(key);
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('q') {
            self.quitting = true;
            return vec![Effect::Quit];
        }
        if self.view == View::Terminal
            && self.editor.is_none()
            && self.terminal.as_ref().is_some_and(TerminalSession::capture_input)
        {
            return self.captured_terminal_key(key);
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('r') {
            return self.request_reconnect();
        }
        if key.code == KeyCode::Esc && self.product.is_some() && self.editor.is_none() {
            self.return_to_conversation();
            return Vec::new();
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('q' | 'c'))
        {
            self.quitting = true;
            return vec![Effect::Quit];
        }
        if self.editor.is_some() {
            return self.handle_editor_key(key);
        }
        if self.view == View::Help
            && crate::help::scroll(&mut self.help_scroll, key, self.chat.viewport)
        {
            return Vec::new();
        }
        if let Some(effects) = self.handle_product_key(key) {
            return effects;
        }
        self.handle_panel_key(key)
    }

    fn handle_panel_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if self.view == View::Approvals
            && matches!(
                key.code,
                KeyCode::PageUp | KeyCode::PageDown | KeyCode::Home | KeyCode::End
            )
        {
            let maximum = crate::render::prompt_scroll_limit(self);
            let current = self.prompt_scroll.min(maximum);
            self.prompt_scroll = match key.code {
                KeyCode::PageUp => current.saturating_sub(12),
                KeyCode::PageDown => current.saturating_add(12).min(maximum),
                KeyCode::End => maximum,
                _ => 0,
            };
            return Vec::new();
        }
        match key.code {
            KeyCode::Char('1') => self.view = View::Runs,
            KeyCode::Char('2') => return self.open_diff_panel(),
            KeyCode::Char('3') => self.view = View::Review,
            KeyCode::Char('4') => self.view = View::Trace,
            KeyCode::Char('5') => self.view = View::Evolution,
            KeyCode::Char('6') => self.view = View::Terminal,
            KeyCode::Char('7') => self.view = View::Approvals,
            KeyCode::Char('8') => self.view = View::Preview,
            KeyCode::Char('9' | '?') => self.view = View::Help,
            KeyCode::Tab => self.next_view(),
            KeyCode::BackTab => self.previous_view(),
            KeyCode::Up | KeyCode::Char('k') => self.select_previous(),
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::PageUp => {
                if let Some(terminal) = &mut self.terminal {
                    terminal.scroll_up();
                }
            }
            KeyCode::PageDown => {
                if let Some(terminal) = &mut self.terminal {
                    terminal.scroll_down();
                }
            }
            KeyCode::Char('r' | 'R') => {
                return self.request_reconnect();
            }
            KeyCode::Char('a')
                if self.view == View::Terminal
                    && !self.terminal.as_ref().is_some_and(TerminalSession::can_capture) =>
            {
                self.open_editor(Editor {
                    kind: EditorKind::ProcessId,
                    title: "Attach to daemon-owned process",
                    hint: "Enter the 32 hexadecimal digits of a ProcessId",
                    buffer: String::new(),
                    cursor: 0,
                    pasted_command: false,
                });
            }
            KeyCode::Char('i') if self.view == View::Terminal => {
                if let Some(terminal) = &mut self.terminal
                    && terminal.can_capture()
                {
                    terminal.set_capture_input(true);
                    self.notice(
                        NoticeLevel::Info,
                        "terminal keyboard capture enabled; Ctrl-] releases it",
                    );
                } else {
                    self.notice(
                        NoticeLevel::Info,
                        "No live terminal attachment; press a to attach after connecting.",
                    );
                }
            }
            KeyCode::Char('d') if self.view == View::Terminal => {
                return self.detach_terminal();
            }
            KeyCode::Char('x') if self.view == View::Terminal => {
                return self.cancel_terminal();
            }
            KeyCode::Enter if self.view == View::Approvals => self.open_prompt_editor(),
            KeyCode::Char('c') if self.view == View::Approvals => {
                return self.cancel_selected_prompt();
            }
            KeyCode::Char('p') if self.subscription.is_some() => {
                return self.subscription_control(true);
            }
            KeyCode::Char('u') if self.subscription.is_some() => {
                return self.subscription_control(false);
            }
            _ => {}
        }
        Vec::new()
    }

    fn return_to_conversation(&mut self) {
        if let Some(terminal) = &mut self.terminal {
            terminal.set_capture_input(false);
        }
        self.view = View::Conversation;
    }

    pub(super) fn request_reconnect(&mut self) -> Vec<Effect> {
        self.connection = ConnectionStatus::Connecting;
        vec![Effect::Reconnect]
    }

    fn subscription_control(&mut self, pause: bool) -> Vec<Effect> {
        let (Some(context), Some(subscription_id), Some(correlation)) =
            (self.context, self.subscription, self.ids.correlation())
        else {
            return Vec::new();
        };
        let payload = if pause {
            SubscriptionControl::Pause { subscription_id, reason: PauseReason::Client }
        } else {
            SubscriptionControl::Resume { subscription_id }
        };
        vec![Effect::Send(AppMessage::Control(ControlEnvelope::new(
            context,
            correlation,
            ControlPayload::Subscription(payload),
        )))]
    }

    fn next_view(&mut self) {
        let index = View::ALL.iter().position(|view| *view == self.view).unwrap_or(0);
        self.view = View::ALL[(index + 1) % View::ALL.len()];
    }

    fn previous_view(&mut self) {
        let index = View::ALL.iter().position(|view| *view == self.view).unwrap_or(0);
        self.view = View::ALL[(index + View::ALL.len() - 1) % View::ALL.len()];
    }

    fn select_previous(&mut self) {
        if self.view == View::Approvals {
            self.prompt_scroll = 0;
            self.selected_prompt = self.selected_prompt.saturating_sub(1);
            return;
        }
        if matches!(self.view, View::Runs | View::Diff | View::Review)
            && self.select_previous_product()
        {
            return;
        }
        let visible = self.visible_event_indices();
        if visible.is_empty() {
            self.selected_event = None;
            return;
        }
        let position = self
            .selected_event
            .and_then(|selected| visible.iter().position(|index| *index == selected))
            .unwrap_or(visible.len());
        self.selected_event = Some(visible[position.saturating_sub(1)]);
    }

    fn select_next(&mut self) {
        if self.view == View::Approvals {
            self.prompt_scroll = 0;
            self.selected_prompt =
                (self.selected_prompt + 1).min(self.prompts.len().saturating_sub(1));
            return;
        }
        if matches!(self.view, View::Runs | View::Diff | View::Review) && self.select_next_product()
        {
            return;
        }
        let visible = self.visible_event_indices();
        if visible.is_empty() {
            self.selected_event = None;
            return;
        }
        let position = self
            .selected_event
            .and_then(|selected| visible.iter().position(|index| *index == selected))
            .map_or(0, |position| (position + 1).min(visible.len() - 1));
        self.selected_event = Some(visible[position]);
    }

    fn prompt(&self, prompt_id: PromptId) -> Option<&PromptItem> {
        self.prompts.iter().find(|item| item.binding.correlation().prompt_id() == prompt_id)
    }

    pub(super) fn set_prompt_phase(&mut self, prompt_id: PromptId, phase: PromptPhase) {
        if let Some(item) =
            self.prompts.iter_mut().find(|item| item.binding.correlation().prompt_id() == prompt_id)
        {
            item.phase = phase;
        }
    }

    pub(super) fn notice(&mut self, level: NoticeLevel, text: impl Into<String>) {
        let mut text = text.into();
        if text.len() > self.limits.max_diagnostic_bytes() {
            let mut end = self.limits.max_diagnostic_bytes();
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
        }
        self.notice = Some(Notice { level, text });
        self.status_scroll = 0;
    }
}
