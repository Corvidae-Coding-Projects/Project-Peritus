//! Terminal capture, attachment and bounded child control requests.

use super::{
    AppMessage, AppModel, AppRequestEnvelope, AppRequestPayload, Effect, KeyCode, KeyEvent,
    KeyModifiers, NoticeLevel, PendingRequest, ProcessId, TerminalBinding, TerminalCancellation,
    TerminalDetach, TerminalInput, TerminalResize, TerminalSession, decode_hex_16,
};

impl AppModel {
    pub(super) fn captured_terminal_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char(']' | '5'))
        {
            if let Some(terminal) = &mut self.terminal {
                terminal.set_capture_input(false);
            }
            self.notice(NoticeLevel::Info, "terminal keyboard capture released");
            return Vec::new();
        }
        self.terminal
            .as_ref()
            .and_then(|terminal| terminal.key_bytes(key))
            .map_or_else(Vec::new, |bytes| self.send_terminal_input(bytes))
    }

    pub(in crate::model) fn attach_terminal(&mut self, process_text: &str) -> Vec<Effect> {
        let Some(context) = self.context else {
            return Vec::new();
        };
        if self.pending.values().any(|request| matches!(request, PendingRequest::TerminalAttach(_)))
        {
            self.notice(
                NoticeLevel::Info,
                "A terminal attachment is already pending; this draft is retained.",
            );
            return Vec::new();
        }
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
        self.track_request(request, PendingRequest::TerminalAttach(binding));
        self.notice(NoticeLevel::Info, "terminal attachment requested");
        vec![Effect::Send(AppMessage::Request(envelope))]
    }

    pub(super) fn send_terminal_input(&mut self, bytes: Vec<u8>) -> Vec<Effect> {
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

    pub(in crate::model) fn send_terminal_resize(
        &mut self,
        columns: u16,
        rows: u16,
    ) -> Vec<Effect> {
        let Some(binding) = self
            .terminal
            .as_ref()
            .filter(|terminal| terminal.can_capture())
            .map(TerminalSession::binding)
        else {
            return Vec::new();
        };
        let columns = columns.saturating_sub(2).clamp(1, 4096);
        let rows = rows.saturating_sub(6).clamp(1, 256);
        let resize = match TerminalResize::new(binding, columns, rows, 4096, 256) {
            Ok(resize) => resize,
            Err(error) => {
                self.notice(NoticeLevel::Error, error.to_string());
                return Vec::new();
            }
        };
        if let Some(terminal) = &mut self.terminal
            && let Err(error) = terminal.resize(resize)
        {
            self.notice(NoticeLevel::Error, error.to_string());
            return Vec::new();
        }
        self.request(AppRequestPayload::TerminalResize(resize), PendingRequest::TerminalResize)
            .into_iter()
            .collect()
    }

    pub(super) fn detach_terminal(&mut self) -> Vec<Effect> {
        if self.terminal.as_ref().is_some_and(|terminal| !terminal.can_capture()) {
            self.terminal = None;
            self.notice(NoticeLevel::Info, "terminal transcript closed");
            return Vec::new();
        }
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
        self.track_request(request, PendingRequest::TerminalDetach(binding));
        vec![Effect::Send(AppMessage::Request(envelope))]
    }

    pub(super) fn cancel_terminal(&mut self) -> Vec<Effect> {
        if !self.terminal.as_ref().is_some_and(TerminalSession::can_capture) {
            self.notice(NoticeLevel::Info, "No live terminal attachment; reconnect and attach the process before requesting cancellation.");
            return Vec::new();
        }
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
        self.track_request(request, PendingRequest::TerminalCancel);
        vec![Effect::Send(AppMessage::Request(envelope))]
    }
}
