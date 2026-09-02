//! Deterministic application state and update reducer.

mod interaction;
mod product;
mod protocol;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet, VecDeque};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use peritus_app_protocol::{
    Acknowledgement, AppEventEnvelope, AppEventPayload, AppMessage, AppProtocolLimits,
    AppRequestEnvelope, AppRequestPayload, AppResponseEnvelope, AppResponsePayload,
    ControlEnvelope, ControlPayload, CorrelationId, EventCursor, HeartbeatReply, PauseReason,
    PromptAnswer, PromptAnswerPayload, PromptBinding, PromptCancellation, PromptId, PromptKind,
    ProtocolContext, RequestId, SignedApprovalDecisionFrame, SubscriptionCancellation,
    SubscriptionCancellationSource, SubscriptionControl, SubscriptionFilter, SubscriptionId,
    SubscriptionRequest, TerminalBinding, TerminalCancellation, TerminalDetach, TerminalInput,
    TerminalResize, UserInputValue,
};
use peritus_protocol::schema::FAMILIES;
use peritus_types::{EventId, ProcessId, RunId, SessionId};
use sha2::{Digest, Sha256};

use crate::{
    action::{Action, Effect},
    input::{edit_text, is_active_key, terminal_bytes},
    runtime::ProductLaunchContext,
    sanitize::inert_preview,
    terminal::TerminalSession,
};
use product::{ProductUi, ProviderRole};

const EVENT_CAPACITY: usize = 4_096;
const NOTICE_TICKS: u16 = 24;

/// Primary full-screen presentation selected by the user.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum View {
    Runs,
    Diff,
    Review,
    Trace,
    Evolution,
    Terminal,
    Approvals,
    Help,
}

impl View {
    pub(crate) const ALL: [Self; 8] = [
        Self::Runs,
        Self::Diff,
        Self::Review,
        Self::Trace,
        Self::Evolution,
        Self::Terminal,
        Self::Approvals,
        Self::Help,
    ];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Runs => "Runs",
            Self::Diff => "Diff",
            Self::Review => "Review",
            Self::Trace => "Trace",
            Self::Evolution => "Evolution",
            Self::Terminal => "Terminal",
            Self::Approvals => "Approvals",
            Self::Help => "Help",
        }
    }
}

/// Observable connection state without implied daemon authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConnectionStatus {
    Connecting,
    Online { server: String, downgraded: bool },
    Disconnected(String),
}

/// Severity of one bounded transient status message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

/// A bounded transient user-visible status message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Notice {
    pub(crate) level: NoticeLevel,
    pub(crate) text: String,
    ticks_remaining: u16,
}

/// One exact delivered B3 frame summarized without executing its content.
#[derive(Clone, Debug)]
pub struct EventRecord {
    pub(crate) event_id: EventId,
    pub(crate) cursor: EventCursor,
    pub(crate) family: u16,
    pub(crate) family_name: &'static str,
    pub(crate) schema: u16,
    pub(crate) attempt: u32,
    pub(crate) digest: [u8; 32],
    pub(crate) byte_len: usize,
    pub(crate) preview: String,
}

impl EventRecord {
    pub(crate) fn summary(&self) -> String {
        format!(
            "#{:<6} {:<27} attempt {}  {} bytes",
            self.cursor.get(),
            self.family_name,
            self.attempt,
            self.byte_len
        )
    }

    pub(crate) const fn visible_in(&self, view: View) -> bool {
        match view {
            View::Runs => matches!(self.family, 3 | 41 | 71 | 74 | 77),
            View::Diff => matches!(self.family, 3 | 41 | 51 | 77 | 80),
            View::Review => matches!(self.family, 51 | 54),
            View::Trace => matches!(self.family, 60 | 83),
            View::Evolution => matches!(self.family, 80 | 86 | 89 | 92),
            View::Terminal | View::Approvals | View::Help => false,
        }
    }
}

/// Local presentation phase of one daemon-owned prompt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromptPhase {
    Pending,
    Submitting,
    Accepted,
    Failed,
}

/// One immutable prompt binding plus local presentation state.
#[derive(Clone, Debug)]
pub struct PromptItem {
    pub(crate) binding: PromptBinding,
    pub(crate) phase: PromptPhase,
}

#[derive(Clone, Debug)]
enum PendingRequest {
    Status,
    Subscribe,
    Prompt(PromptId),
    TerminalAttach,
    TerminalInput,
    TerminalResize,
    TerminalDetach,
    TerminalCancel,
    ProductStart,
    ProductQuery,
    ProductControl,
    ProductContinue,
    ProductConversationQuery,
}

/// The kind of value being collected by the modal editor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditorKind {
    ProcessId,
    ApprovalSignature(PromptId),
    PromptAnswer(PromptId),
    ProductTask,
    ProductMessage(RunId),
}

/// Modal, single-line input state.
#[derive(Clone, Debug)]
pub struct Editor {
    pub(crate) kind: EditorKind,
    pub(crate) title: &'static str,
    pub(crate) hint: &'static str,
    pub(crate) buffer: String,
    pub(crate) cursor: usize,
}

#[derive(Debug)]
struct IdFactory {
    seed: [u8; 32],
    counter: u64,
}

impl IdFactory {
    const fn new(seed: [u8; 32]) -> Self {
        Self { seed, counter: 0 }
    }

    fn bytes(&mut self, domain: &[u8]) -> [u8; 16] {
        self.counter = self.counter.saturating_add(1);
        let mut hasher = Sha256::new();
        hasher.update(b"peritus/tui-identity/v1\0");
        hasher.update(domain);
        hasher.update(self.seed);
        hasher.update(self.counter.to_be_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        bytes[0] |= 1;
        bytes
    }

    fn request(&mut self) -> Option<RequestId> {
        RequestId::new(self.bytes(b"request")).ok()
    }

    fn correlation(&mut self) -> Option<CorrelationId> {
        CorrelationId::new(self.bytes(b"correlation")).ok()
    }

    fn subscription(&mut self) -> Option<SubscriptionId> {
        SubscriptionId::new(self.bytes(b"subscription")).ok()
    }

    fn attachment(&mut self) -> Option<peritus_app_protocol::TerminalAttachmentId> {
        peritus_app_protocol::TerminalAttachmentId::new(self.bytes(b"terminal-attachment")).ok()
    }

    fn run(&mut self) -> Option<RunId> {
        RunId::new(self.bytes(b"product-run")).ok()
    }
}

/// Complete deterministic client presentation state.
#[derive(Debug)]
pub struct AppModel {
    pub(crate) view: View,
    pub(crate) connection: ConnectionStatus,
    pub(crate) daemon_status: Option<peritus_app_protocol::DaemonStatus>,
    pub(crate) events: VecDeque<EventRecord>,
    seen_events: HashSet<EventId>,
    pub(crate) selected_event: Option<usize>,
    pub(crate) prompts: Vec<PromptItem>,
    pub(crate) selected_prompt: usize,
    pub(crate) terminal: Option<TerminalSession>,
    pub(crate) notice: Option<Notice>,
    pub(crate) editor: Option<Editor>,
    pub(crate) quitting: bool,
    context: Option<ProtocolContext>,
    limits: AppProtocolLimits,
    subscription: Option<SubscriptionId>,
    last_cursor: EventCursor,
    pending: HashMap<RequestId, PendingRequest>,
    ids: IdFactory,
    pub(super) product: Option<ProductUi>,
    tick_count: u64,
}

impl AppModel {
    #[cfg(test)]
    pub(crate) fn new(seed: [u8; 32]) -> Self {
        Self::with_product(seed, None)
    }

    pub(crate) fn with_product(seed: [u8; 32], product: Option<ProductLaunchContext>) -> Self {
        Self {
            view: View::Runs,
            connection: ConnectionStatus::Connecting,
            daemon_status: None,
            events: VecDeque::new(),
            seen_events: HashSet::new(),
            selected_event: None,
            prompts: Vec::new(),
            selected_prompt: 0,
            terminal: None,
            notice: None,
            editor: None,
            quitting: false,
            context: None,
            limits: AppProtocolLimits::PRODUCTION,
            subscription: None,
            last_cursor: EventCursor::origin(),
            pending: HashMap::new(),
            ids: IdFactory::new(seed),
            product: product.map(ProductUi::new),
            tick_count: 0,
        }
    }

    pub(crate) fn update(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::Connected { context, limits, server, downgraded } => {
                self.context = Some(context);
                self.limits = limits;
                self.connection = ConnectionStatus::Online { server, downgraded };
                self.notice(NoticeLevel::Info, "connected to daemon");
                self.start_session()
            }
            Action::ConnectionFailed(error) | Action::Disconnected(error) => {
                self.connection = ConnectionStatus::Disconnected(error.clone());
                self.context = None;
                self.pending.clear();
                self.notice(NoticeLevel::Error, format!("daemon disconnected: {error}"));
                Vec::new()
            }
            Action::Message(message) => self.handle_message(message),
            Action::TerminalEvent(event) => self.handle_terminal_event(event),
            Action::Tick => {
                self.tick_count = self.tick_count.saturating_add(1);
                if let Some(notice) = &mut self.notice {
                    notice.ticks_remaining = notice.ticks_remaining.saturating_sub(1);
                    if notice.ticks_remaining == 0 {
                        self.notice = None;
                    }
                }
                if self.tick_count.is_multiple_of(4) {
                    self.poll_product_runs()
                } else {
                    Vec::new()
                }
            }
        }
    }

    pub(crate) fn visible_event_indices(&self) -> Vec<usize> {
        self.events
            .iter()
            .enumerate()
            .filter_map(|(index, event)| event.visible_in(self.view).then_some(index))
            .collect()
    }

    pub(crate) fn selected_event_record(&self) -> Option<&EventRecord> {
        self.selected_event.and_then(|index| self.events.get(index))
    }

    pub(crate) fn selected_prompt_item(&self) -> Option<&PromptItem> {
        self.prompts.get(self.selected_prompt)
    }

    pub(crate) fn session_label(&self) -> String {
        session_label(self.context)
    }

    pub(crate) const fn last_cursor(&self) -> EventCursor {
        self.last_cursor
    }

    pub(crate) fn retained_session(&self) -> Option<SessionId> {
        self.context.map(ProtocolContext::session_id)
    }

    pub(crate) fn cleanup_messages(&mut self) -> Vec<AppMessage> {
        let Some(context) = self.context else {
            return Vec::new();
        };
        let mut messages = Vec::new();
        if let Some(subscription_id) = self.subscription.take()
            && let Some(correlation) = self.ids.correlation()
        {
            messages.push(AppMessage::Control(ControlEnvelope::new(
                context,
                correlation,
                ControlPayload::CancelSubscription(SubscriptionCancellation::new(
                    subscription_id,
                    correlation,
                    SubscriptionCancellationSource::Client,
                )),
            )));
        }
        if let Some(binding) = self.terminal.as_ref().map(TerminalSession::binding)
            && let (Some(request), Some(correlation)) = (self.ids.request(), self.ids.correlation())
            && let Ok(envelope) = AppRequestEnvelope::new(
                context,
                request,
                correlation,
                AppRequestPayload::DetachTerminal(TerminalDetach::new(binding, correlation)),
            )
        {
            messages.push(AppMessage::Request(envelope));
        }
        messages
    }
}

fn decode_hex_16(text: &str) -> Option<[u8; 16]> {
    if text.len() != 32 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut output = [0_u8; 16];
    for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(pair[0])?;
        let low = hex_nibble(pair[1])?;
        output[index] = high << 4 | low;
    }
    Some(output)
}

const fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub fn format_id(bytes: &[u8; 16]) -> String {
    let mut output = String::with_capacity(32);
    for byte in bytes {
        use core::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

pub fn format_digest(bytes: &[u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in bytes {
        use core::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

pub fn session_label(context: Option<ProtocolContext>) -> String {
    context
        .map_or_else(|| "no session".to_owned(), |value| format_id(value.session_id().as_bytes()))
}
