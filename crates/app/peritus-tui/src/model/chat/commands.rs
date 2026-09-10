//! Small discoverable command vocabulary; unknown commands never reach the model.

use super::catalog::{self, Command};
use crate::model::{AppModel, Effect, NoticeLevel, View};
use peritus_app_protocol::{ProductInteractionMode as Mode, ProductRunControlAction as Control};

impl AppModel {
    pub(super) fn slash_command(&mut self, text: &str) -> Vec<Effect> {
        self.chat.workbench.files.open = false;
        let (command, rest) = match catalog::parse(text) {
            Ok(parsed) => parsed,
            Err(message) => {
                self.notice(NoticeLevel::Warning, message);
                return Vec::new();
            }
        };
        if !matches!(
            command,
            Command::Goal | Command::Pause | Command::Resume | Command::Usage | Command::Budget
        ) {
            self.chat.workbench.goal_mode = false;
        }
        if let Some(effects) = self.workbench_slash_command(command, rest) {
            return effects;
        }
        match command {
            Command::Model => return self.model_command(rest),
            Command::Effort => return self.effort_command(rest),
            Command::Doctor => return self.doctor_command(),
            _ => {}
        }
        if self.direct_folder_chat().is_some()
            && matches!(
                command,
                Command::Build
                    | Command::Accept
                    | Command::Commit
                    | Command::Export
                    | Command::Discard
                    | Command::Run
            )
        {
            self.notice(NoticeLevel::Info, "This folder uses in-place edits, not Git candidate handoffs. Ask for changes or commands in /chat; /build and candidate actions require a managed Git workspace.");
            return Vec::new();
        }
        if command == Command::New && self.chat_submission_pending() {
            self.notice(
                NoticeLevel::Info,
                "Wait for the pending input receipt before opening a new conversation.",
            );
            return Vec::new();
        }
        let mode = match command {
            Command::Chat => Some(Mode::Chat),
            Command::Plan => Some(Mode::Plan),
            Command::Review => Some(Mode::Review),
            Command::Build => Some(Mode::Build),
            _ => None,
        };
        if let Some(mode) = mode {
            return self.chat_mode_command(mode, rest);
        }
        self.clear_chat_command();
        match command {
            Command::New => {
                self.chat.run_id = None;
                self.chat.snapshot = None;
                self.chat.scroll = 0;
                self.chat.mode = Mode::Chat;
            }
            Command::Status => self.notice(NoticeLevel::Info, self.chat.status()),
            Command::Diff => {
                if self.select_chat_run() {
                    return self.open_diff_panel();
                }
            }
            Command::Runs => self.view = View::Runs,
            Command::Trace => self.view = View::Trace,
            Command::Terminal => self.view = View::Terminal,
            Command::Approvals => self.view = View::Approvals,
            Command::Details => self.chat.expanded = !self.chat.expanded,
            Command::Stop => return self.chat_control(Control::Cancel),
            Command::Accept => return self.chat_control(Control::Accept),
            Command::Commit => return self.chat_control(Control::Commit),
            Command::Export => return self.chat_control(Control::Export),
            Command::Discard => return self.chat_control(Control::Discard),
            Command::Run => {
                if self.select_chat_run() {
                    return self.run_selected_product_candidate();
                }
            }
            Command::Reconnect => return vec![Effect::Reconnect],
            Command::Help => {
                "/".clone_into(&mut self.chat.buffer);
                self.chat.cursor = 1;
            }
            Command::Quit => {
                self.quitting = true;
                return vec![Effect::Quit];
            }
            _ => {}
        }
        Vec::new()
    }

    fn workbench_slash_command(&mut self, command: Command, rest: &str) -> Option<Vec<Effect>> {
        match command {
            Command::Sessions => Some(self.sessions_command(rest)),
            Command::Fork => Some(self.fork_command(rest)),
            Command::Queue => Some(self.queue_command(rest)),
            Command::Context => Some(self.context_command(rest)),
            Command::Compact => Some(self.compact_command(rest)),
            Command::Brief => Some(self.brief_command(rest)),
            Command::Goal => Some(self.goal_command(rest)),
            Command::Pause => Some(self.pause_goal_command(rest)),
            Command::Resume => Some(self.resume_goal_command(rest)),
            Command::Usage => Some(self.usage_command()),
            Command::Budget => Some(self.budget_command(rest)),
            Command::Preview => Some(self.preview_command(rest)),
            Command::Checkpoint => Some(self.checkpoint_command(rest)),
            Command::Rewind => Some(self.rewind_command(rest)),
            Command::Attach => Some(self.image_command(rest)),
            Command::Files => Some(self.file_command(rest)),
            Command::Permissions => Some(self.permissions_command(rest)),
            Command::Init => Some(self.init_command(rest)),
            Command::Memory => Some(self.memory_command(rest)),
            _ => None,
        }
    }

    fn chat_mode_command(&mut self, mode: Mode, text: &str) -> Vec<Effect> {
        if self.chat_work_active() && self.chat.mode != mode {
            self.notice(
                NoticeLevel::Warning,
                "Stop active work before changing mode. Your draft is retained.",
            );
            return Vec::new();
        }
        self.chat.mode = mode;
        if text.is_empty() {
            self.clear_chat_command();
            self.notice(NoticeLevel::Info, format!("{} selected", mode.label()));
            Vec::new()
        } else {
            self.send_chat_message(text.to_owned())
        }
    }
    pub(in crate::model) fn clear_chat_command(&mut self) {
        self.chat.pasted_command = false;
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
