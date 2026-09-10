//! Small discoverable command vocabulary; unknown commands never reach the model.

use crate::model::{AppModel, Effect, NoticeLevel, View};
use peritus_app_protocol::{ProductInteractionMode as Mode, ProductRunControlAction as Control};

pub const COMMANDS: &[(&str, &str)] = &[
    ("/chat", "Talk or request scoped work"),
    ("/plan", "Read-only planning"),
    ("/review", "Fresh independent read-only review"),
    ("/build", "Checked writer / reviewer / fixer delivery"),
    ("/model", "Discover and select provider models"),
    ("/effort", "Select reasoning effort for chat/writer, reviewer, or fixer"),
    ("/new", "New conversation; preserve existing work"),
    ("/status", "Current conversation and input status"),
    ("/diff", "Inspect retained workspace changes"),
    ("/runs", "Browse runs and handoffs"),
    ("/trace", "Inspect event trace"),
    ("/terminal", "Attach to a daemon process"),
    ("/approvals", "Inspect pending approvals"),
    ("/details", "Show or hide harness diagnostics"),
    ("/stop", "Stop active work, preserve effects"),
    ("/accept", "Accept the exact candidate"),
    ("/commit", "Commit the exact candidate"),
    ("/export", "Export the exact candidate"),
    ("/discard", "Discard the exact candidate"),
    ("/run", "Run the retained candidate"),
    ("/reconnect", "Reconnect to the daemon"),
    ("/help", "Show slash commands"),
    ("/quit", "Detach client; do not stop daemon work"),
];
impl AppModel {
    pub(super) fn slash_command(&mut self, text: &str) -> Vec<Effect> {
        let (command, rest) = text
            .split_once(char::is_whitespace)
            .map_or((text, ""), |(command, rest)| (command, rest.trim()));
        if !COMMANDS.iter().any(|(name, _)| *name == command) {
            self.notice(
                NoticeLevel::Warning,
                "Unknown slash command. Type / to see available commands.",
            );
            return Vec::new();
        }
        if command == "/model" {
            return self.model_command(rest);
        }
        if command == "/effort" {
            return self.effort_command(rest);
        }
        if self.direct_folder_chat().is_some()
            && matches!(command, "/build" | "/accept" | "/commit" | "/export" | "/discard" | "/run")
        {
            self.notice(NoticeLevel::Info, "This folder uses in-place edits, not Git candidate handoffs. Ask for changes or commands in /chat; /build and candidate actions require a managed Git workspace.");
            return Vec::new();
        }
        if command == "/new" && self.chat_submission_pending() {
            self.notice(
                NoticeLevel::Info,
                "Wait for the pending input receipt before opening a new conversation.",
            );
            return Vec::new();
        }
        let mode = match command {
            "/chat" => Some(Mode::Chat),
            "/plan" => Some(Mode::Plan),
            "/review" => Some(Mode::Review),
            "/build" => Some(Mode::Build),
            _ => None,
        };
        if let Some(mode) = mode {
            if self.chat_work_active() && self.chat.mode != mode {
                self.notice(
                    NoticeLevel::Warning,
                    "Stop active work before changing mode. Your draft is retained.",
                );
                return Vec::new();
            }
            self.chat.mode = mode;
            if rest.is_empty() {
                self.clear_chat_command();
                self.notice(NoticeLevel::Info, format!("{} selected", mode.label()));
                return Vec::new();
            }
            return self.send_chat_message(rest.to_owned());
        }
        if !rest.is_empty() {
            self.notice(NoticeLevel::Warning, "This command takes no arguments; draft retained.");
            return Vec::new();
        }
        self.clear_chat_command();
        match command {
            "/new" => {
                self.chat.run_id = None;
                self.chat.snapshot = None;
                self.chat.scroll = 0;
                self.chat.mode = Mode::Chat;
            }
            "/status" => self.notice(NoticeLevel::Info, self.chat.status()),
            "/diff" => {
                if self.select_chat_run() {
                    if let Some(product) = &mut self.product {
                        product.inspection_scroll = 0;
                    }
                    self.view = View::Diff;
                }
            }
            "/runs" => self.view = View::Runs,
            "/trace" => self.view = View::Trace,
            "/terminal" => self.view = View::Terminal,
            "/approvals" => self.view = View::Approvals,
            "/details" => self.chat.expanded = !self.chat.expanded,
            "/stop" => return self.chat_control(Control::Cancel),
            "/accept" => return self.chat_control(Control::Accept),
            "/commit" => return self.chat_control(Control::Commit),
            "/export" => return self.chat_control(Control::Export),
            "/discard" => return self.chat_control(Control::Discard),
            "/run" => {
                if self.select_chat_run() {
                    return self.run_selected_product_candidate();
                }
            }
            "/reconnect" => return vec![Effect::Reconnect],
            "/help" => {
                "/".clone_into(&mut self.chat.buffer);
                self.chat.cursor = 1;
            }
            "/quit" => {
                self.quitting = true;
                return vec![Effect::Quit];
            }
            _ => {}
        }
        Vec::new()
    }
    pub(super) fn clear_chat_command(&mut self) {
        self.chat.buffer.clear();
        self.chat.cursor = 0;
        self.chat.command_selection = 0;
    }

    pub(crate) fn direct_folder_chat(&self) -> Option<bool> {
        let product = self.product.as_ref()?;
        if self.chat.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.snapshot().workspace_id() != product.launch.workspace_id()
        }) {
            return None;
        }
        product.launch.direct_folder_writable()
    }
}
