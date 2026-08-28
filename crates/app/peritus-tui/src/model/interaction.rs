//! Keyboard interaction and request construction for mutable UI workflows.

use super::*;

impl AppModel {
    pub(super) fn handle_terminal_event(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Paste(text) => {
                if let Some(editor) = &mut self.editor {
                    editor.buffer.insert_str(editor.cursor, &text);
                    editor.cursor += text.len();
                    Vec::new()
                } else if self.terminal.as_ref().is_some_and(TerminalSession::capture_input) {
                    self.send_terminal_input(text.into_bytes())
                } else {
                    Vec::new()
                }
            }
            Event::Resize(columns, rows) => self.send_terminal_resize(columns, rows),
            Event::FocusGained | Event::FocusLost | Event::Mouse(_) => Vec::new(),
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if !is_active_key(key) {
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
        if self.view == View::Terminal
            && self.terminal.as_ref().is_some_and(TerminalSession::capture_input)
        {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char(']') {
                if let Some(terminal) = &mut self.terminal {
                    terminal.set_capture_input(false);
                }
                self.notice(NoticeLevel::Info, "terminal keyboard capture released");
                return Vec::new();
            }
            return terminal_bytes(key)
                .map_or_else(Vec::new, |bytes| self.send_terminal_input(bytes));
        }
        if let Some(effects) = self.handle_product_key(key) {
            return effects;
        }

        match key.code {
            KeyCode::Char('1') => self.view = View::Runs,
            KeyCode::Char('2') => self.view = View::Diff,
            KeyCode::Char('3') => self.view = View::Review,
            KeyCode::Char('4') => self.view = View::Trace,
            KeyCode::Char('5') => self.view = View::Evolution,
            KeyCode::Char('6') => self.view = View::Terminal,
            KeyCode::Char('7') => self.view = View::Approvals,
            KeyCode::Char('?') => self.view = View::Help,
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
                self.connection = ConnectionStatus::Connecting;
                return vec![Effect::Reconnect];
            }
            KeyCode::Char('a') if self.view == View::Terminal && self.terminal.is_none() => {
                self.editor = Some(Editor {
                    kind: EditorKind::ProcessId,
                    title: "Attach to daemon-owned process",
                    hint: "Enter the 32 hexadecimal digits of a ProcessId",
                    buffer: String::new(),
                    cursor: 0,
                });
            }
            KeyCode::Char('i') if self.view == View::Terminal => {
                if let Some(terminal) = &mut self.terminal {
                    terminal.set_capture_input(true);
                    self.notice(
                        NoticeLevel::Info,
                        "terminal keyboard capture enabled; Ctrl-] releases it",
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

    fn handle_product_key(&mut self, key: KeyEvent) -> Option<Vec<Effect>> {
        if self.view != View::Runs {
            return None;
        }
        match key.code {
            KeyCode::Char('n') => self.open_task_composer(),
            KeyCode::Enter | KeyCode::Char('m') => self.open_product_message_composer(),
            KeyCode::Char('x') => {
                return Some(self.control_selected_product_run(
                    peritus_app_protocol::ProductRunControlAction::Cancel,
                ));
            }
            KeyCode::Char('r') if self.product.is_some() => {
                return Some(self.control_selected_product_run(
                    peritus_app_protocol::ProductRunControlAction::Retry,
                ));
            }
            KeyCode::Char('w') => self.cycle_product_provider(ProviderRole::Writer),
            KeyCode::Char('e') => self.cycle_product_provider(ProviderRole::Reviewer),
            KeyCode::Char('f') => self.cycle_product_provider(ProviderRole::Fixer),
            _ => return None,
        }
        Some(Vec::new())
    }

    fn handle_editor_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if key.code == KeyCode::Esc {
            self.editor = None;
            return Vec::new();
        }
        if key.code == KeyCode::Enter && key.modifiers.contains(KeyModifiers::SHIFT) {
            if let Some(editor) = &mut self.editor
                && matches!(editor.kind, EditorKind::ProductTask | EditorKind::ProductMessage(_))
            {
                editor.buffer.insert(editor.cursor, '\n');
                editor.cursor += 1;
            }
            return Vec::new();
        }
        if key.code == KeyCode::Enter {
            let Some(editor) = self.editor.take() else {
                return Vec::new();
            };
            return self.submit_editor(editor);
        }
        if let Some(editor) = &mut self.editor {
            let _ = edit_text(&mut editor.buffer, &mut editor.cursor, key);
        }
        Vec::new()
    }

    fn submit_editor(&mut self, editor: Editor) -> Vec<Effect> {
        match editor.kind {
            EditorKind::ProcessId => self.attach_terminal(&editor.buffer),
            EditorKind::ApprovalSignature(prompt_id) => {
                self.submit_signed_approval(prompt_id, &editor.buffer)
            }
            EditorKind::PromptAnswer(prompt_id) => self.submit_user_input(prompt_id, editor.buffer),
            EditorKind::ProductTask => self.submit_product_task(editor.buffer),
            EditorKind::ProductMessage(run_id) => {
                self.submit_product_message(run_id, editor.buffer)
            }
        }
    }

    fn open_prompt_editor(&mut self) {
        let Some(item) = self.selected_prompt_item() else {
            return;
        };
        if item.phase != PromptPhase::Pending {
            return;
        }
        let prompt_id = item.binding.correlation().prompt_id();
        self.editor = Some(match item.binding.kind() {
            PromptKind::Approval => Editor {
                kind: EditorKind::ApprovalSignature(prompt_id),
                title: "Submit externally signed approval decision",
                hint: "Paste the base64-encoded canonical B1 signed-decision frame",
                buffer: String::new(),
                cursor: 0,
            },
            PromptKind::UserInput => Editor {
                kind: EditorKind::PromptAnswer(prompt_id),
                title: "Answer daemon prompt",
                hint: "Enter text, an exact choice id, or an opaque secret reference as required",
                buffer: String::new(),
                cursor: 0,
            },
        });
    }

    fn submit_signed_approval(&mut self, prompt_id: PromptId, encoded: &str) -> Vec<Effect> {
        let decoded = match BASE64.decode(encoded.trim()) {
            Ok(decoded) => decoded,
            Err(error) => {
                self.notice(NoticeLevel::Error, format!("invalid base64 decision: {error}"));
                return Vec::new();
            }
        };
        let decision =
            match SignedApprovalDecisionFrame::new(decoded, self.limits.codec().max_frame_bytes) {
                Ok(decision) => decision,
                Err(error) => {
                    self.notice(NoticeLevel::Error, format!("invalid signed decision: {error}"));
                    return Vec::new();
                }
            };
        let payload = match PromptAnswerPayload::signed_approval(
            decision,
            None,
            self.limits.max_diagnostic_bytes(),
        ) {
            Ok(payload) => payload,
            Err(error) => {
                self.notice(NoticeLevel::Error, error.to_string());
                return Vec::new();
            }
        };
        self.submit_prompt_answer(prompt_id, payload)
    }

    fn submit_user_input(&mut self, prompt_id: PromptId, value: String) -> Vec<Effect> {
        let Some(item) = self.prompt(prompt_id) else {
            return Vec::new();
        };
        let maximum = self.limits.codec().max_string_bytes;
        let input = if item
            .binding
            .constraints()
            .contains(&peritus_app_protocol::PromptConstraint::SecretReference)
        {
            UserInputValue::secret_reference(value, maximum)
        } else if !item.binding.choices().is_empty()
            || item
                .binding
                .constraints()
                .contains(&peritus_app_protocol::PromptConstraint::BoundChoiceOnly)
        {
            UserInputValue::selection(value, maximum)
        } else {
            UserInputValue::text(value, maximum)
        };
        match input {
            Ok(input) => {
                self.submit_prompt_answer(prompt_id, PromptAnswerPayload::UserInput(input))
            }
            Err(error) => {
                self.notice(NoticeLevel::Error, error.to_string());
                Vec::new()
            }
        }
    }

    fn submit_prompt_answer(
        &mut self,
        prompt_id: PromptId,
        payload: PromptAnswerPayload,
    ) -> Vec<Effect> {
        let Some(binding) = self.prompt(prompt_id).map(|item| item.binding.clone()) else {
            return Vec::new();
        };
        let answer = match PromptAnswer::new(
            binding.correlation(),
            payload,
            self.limits.codec().max_string_bytes,
        ) {
            Ok(answer) => answer,
            Err(error) => {
                self.notice(NoticeLevel::Error, error.to_string());
                return Vec::new();
            }
        };
        self.set_prompt_phase(prompt_id, PromptPhase::Submitting);
        self.request(AppRequestPayload::AnswerPrompt(answer), PendingRequest::Prompt(prompt_id))
            .into_iter()
            .collect()
    }

    fn cancel_selected_prompt(&mut self) -> Vec<Effect> {
        let Some(binding) = self.selected_prompt_item().map(|item| item.binding.clone()) else {
            return Vec::new();
        };
        let prompt_id = binding.correlation().prompt_id();
        if binding.kind() == PromptKind::Approval {
            let payload = match PromptAnswerPayload::cancel_approval(
                None,
                self.limits.max_diagnostic_bytes(),
            ) {
                Ok(payload) => payload,
                Err(error) => {
                    self.notice(NoticeLevel::Error, error.to_string());
                    return Vec::new();
                }
            };
            return self.submit_prompt_answer(prompt_id, payload);
        }
        let Some(context) = self.context else {
            return Vec::new();
        };
        let (Some(request), Some(correlation)) = (self.ids.request(), self.ids.correlation())
        else {
            return Vec::new();
        };
        let cancellation = PromptCancellation::new(binding.correlation(), correlation);
        let Ok(envelope) = AppRequestEnvelope::new(
            context,
            request,
            correlation,
            AppRequestPayload::CancelPrompt(cancellation),
        ) else {
            return Vec::new();
        };
        self.pending.insert(request, PendingRequest::Prompt(prompt_id));
        self.set_prompt_phase(prompt_id, PromptPhase::Submitting);
        vec![Effect::Send(AppMessage::Request(envelope))]
    }

    fn attach_terminal(&mut self, process_text: &str) -> Vec<Effect> {
        let Some(context) = self.context else {
            return Vec::new();
        };
        let Some(process_bytes) = decode_hex_16(process_text.trim()) else {
            self.notice(NoticeLevel::Error, "ProcessId must contain 32 hexadecimal digits");
            return Vec::new();
        };
        let process = match ProcessId::new(process_bytes) {
            Ok(process) => process,
            Err(error) => {
                self.notice(NoticeLevel::Error, format!("invalid ProcessId: {error:?}"));
                return Vec::new();
            }
        };
        let (Some(request), Some(correlation), Some(attachment)) =
            (self.ids.request(), self.ids.correlation(), self.ids.attachment())
        else {
            return Vec::new();
        };
        let binding = TerminalBinding::new(attachment, process, request);
        let Ok(envelope) = AppRequestEnvelope::new(
            context,
            request,
            correlation,
            AppRequestPayload::AttachTerminal(binding),
        ) else {
            return Vec::new();
        };
        self.pending.insert(request, PendingRequest::TerminalAttach);
        self.notice(NoticeLevel::Info, "terminal attachment requested");
        vec![Effect::Send(AppMessage::Request(envelope))]
    }

    fn send_terminal_input(&mut self, bytes: Vec<u8>) -> Vec<Effect> {
        let Some(binding) = self.terminal.as_ref().map(TerminalSession::binding) else {
            return Vec::new();
        };
        let input = match TerminalInput::new(binding, bytes, self.limits.max_terminal_chunk_bytes())
        {
            Ok(input) => input,
            Err(error) => {
                self.notice(NoticeLevel::Error, error.to_string());
                return Vec::new();
            }
        };
        if let Some(terminal) = &self.terminal
            && let Err(error) = terminal.validate_input(&input)
        {
            self.notice(NoticeLevel::Error, error.to_string());
            return Vec::new();
        }
        self.request(AppRequestPayload::TerminalInput(input), PendingRequest::TerminalInput)
            .into_iter()
            .collect()
    }

    fn send_terminal_resize(&mut self, columns: u16, rows: u16) -> Vec<Effect> {
        let Some(binding) = self.terminal.as_ref().map(TerminalSession::binding) else {
            return Vec::new();
        };
        let resize = match TerminalResize::new(binding, columns, rows, u16::MAX, u16::MAX) {
            Ok(resize) => resize,
            Err(error) => {
                self.notice(NoticeLevel::Error, error.to_string());
                return Vec::new();
            }
        };
        if let Some(terminal) = &self.terminal
            && let Err(error) = terminal.resize(resize)
        {
            self.notice(NoticeLevel::Error, error.to_string());
            return Vec::new();
        }
        self.request(AppRequestPayload::TerminalResize(resize), PendingRequest::TerminalResize)
            .into_iter()
            .collect()
    }

    fn detach_terminal(&mut self) -> Vec<Effect> {
        let (Some(context), Some(binding)) =
            (self.context, self.terminal.as_ref().map(TerminalSession::binding))
        else {
            return Vec::new();
        };
        let (Some(request), Some(correlation)) = (self.ids.request(), self.ids.correlation())
        else {
            return Vec::new();
        };
        let Ok(envelope) = AppRequestEnvelope::new(
            context,
            request,
            correlation,
            AppRequestPayload::DetachTerminal(TerminalDetach::new(binding, correlation)),
        ) else {
            return Vec::new();
        };
        self.pending.insert(request, PendingRequest::TerminalDetach);
        vec![Effect::Send(AppMessage::Request(envelope))]
    }

    fn cancel_terminal(&mut self) -> Vec<Effect> {
        let (Some(context), Some(binding)) =
            (self.context, self.terminal.as_ref().map(TerminalSession::binding))
        else {
            return Vec::new();
        };
        let (Some(request), Some(correlation)) = (self.ids.request(), self.ids.correlation())
        else {
            return Vec::new();
        };
        let Ok(envelope) = AppRequestEnvelope::new(
            context,
            request,
            correlation,
            AppRequestPayload::CancelTerminal(TerminalCancellation::new(binding, correlation)),
        ) else {
            return Vec::new();
        };
        self.pending.insert(request, PendingRequest::TerminalCancel);
        vec![Effect::Send(AppMessage::Request(envelope))]
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
            text.truncate(self.limits.max_diagnostic_bytes());
        }
        self.notice = Some(Notice { level, text, ticks_remaining: NOTICE_TICKS });
    }
}
