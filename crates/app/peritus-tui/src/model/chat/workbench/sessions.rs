//! Conversation-library navigation and metadata commands.

use super::WorkbenchMode;
use crate::model::{AppModel, Effect, NoticeLevel, PendingRequest, decode_hex_16};
use peritus_app_protocol::{
    AppRequestPayload, ConversationId, ConversationLibraryQuery, ConversationSearchText,
    ConversationTitle, WorkbenchIntent, WorkbenchQuery,
};

impl AppModel {
    pub(in crate::model::chat) fn sessions_command(&mut self, arguments: &str) -> Vec<Effect> {
        if !self.workbench_available() {
            self.notice(NoticeLevel::Warning, "Workbench controls unavailable/offline; /reconnect or upgrade daemon. Draft retained.");
            return Vec::new();
        }
        let (action, text) = arguments
            .split_once(char::is_whitespace)
            .map_or((arguments, ""), |(action, text)| (action, text.trim()));
        self.chat.workbench.mode = WorkbenchMode::Sessions;
        self.chat.workbench.context_mode = None;
        self.chat.workbench.goal_mode = false;
        self.chat.workbench.images.open = false;
        self.chat.workbench.files.open = false;
        let metadata_action = matches!(
            action,
            "new" | "open" | "rename" | "pin" | "unpin" | "archive" | "unarchive" | "retry"
        );
        if !metadata_action {
            if arguments.is_empty() && !self.library_available() {
                self.chat.workbench.open = true;
                return self.refresh_workbench();
            }
            return self.query_conversation_library(arguments);
        }
        if action == "retry" && text.is_empty() {
            return self.retry_workbench();
        }
        if self.chat.workbench.unresolved.is_some() || self.workbench_request_pending() {
            self.notice(
                NoticeLevel::Warning,
                "Resolve the pending control receipt before another change; draft retained.",
            );
            return Vec::new();
        }
        let Some(workspace) = self.product.as_ref().map(|product| product.launch.workspace_id())
        else {
            self.notice(NoticeLevel::Warning, "Select a workspace first; draft retained.");
            return Vec::new();
        };
        if action == "open" {
            let Some(id) = decode_hex_16(text).and_then(|bytes| ConversationId::new(bytes).ok())
            else {
                self.notice(
                    NoticeLevel::Warning,
                    "Use /sessions open <32 hexadecimal conversation ID>; draft retained.",
                );
                return Vec::new();
            };
            self.chat.workbench.selected = Some(WorkbenchQuery::new(id, workspace));
            self.chat.workbench.snapshot = None;
            self.chat.workbench.queue = None;
            self.chat.workbench.context_page = None;
            self.chat.workbench.compaction_request = None;
            self.chat.workbench.compaction_preview = None;
            self.chat.workbench.brief = None;
            self.chat.workbench.goal = None;
            self.chat.workbench.goal_draft = None;
            self.chat.workbench.goal_clear_pending = false;
            self.chat.workbench.checkpoint_receipt = None;
            self.chat.workbench.rewind_request = None;
            self.chat.workbench.rewind_preview = None;
            self.chat.workbench.restore_receipt = None;
            self.chat.workbench.permissions = None;
            self.chat.workbench.init = None;
            self.chat.workbench.memory = None;
            self.chat.workbench.images.page = None;
            self.chat.workbench.open = true;
            return self.refresh_workbench();
        }
        let intent = match (action, text) {
            ("new", title) => {
                ConversationTitle::new(title.to_owned()).map(WorkbenchIntent::CreateConversation)
            }
            ("rename", title) => {
                ConversationTitle::new(title.to_owned()).map(WorkbenchIntent::RenameConversation)
            }
            ("pin", "") => Ok(WorkbenchIntent::PinConversation(true)),
            ("unpin", "") => Ok(WorkbenchIntent::PinConversation(false)),
            ("archive", "") => Ok(WorkbenchIntent::ArchiveConversation(true)),
            ("unarchive", "") => Ok(WorkbenchIntent::ArchiveConversation(false)),
            _ => {
                self.notice(NoticeLevel::Warning, "Use /sessions [new <title> | open <id> | rename <title> | pin | unpin | archive | unarchive | retry]; draft retained.");
                return Vec::new();
            }
        };
        let Ok(intent) = intent else {
            self.notice(
                NoticeLevel::Warning,
                "Title must be 1–256 bytes without control characters; draft retained.",
            );
            return Vec::new();
        };
        self.submit_workbench(intent, workspace)
    }

    fn query_conversation_library(&mut self, literal: &str) -> Vec<Effect> {
        if !self.library_available() || self.workbench_request_pending() {
            self.notice(
                NoticeLevel::Warning,
                "Conversation library unavailable/offline; /reconnect or upgrade daemon.",
            );
            return Vec::new();
        }
        let Some(workspace) = self.product.as_ref().map(|product| product.launch.workspace_id())
        else {
            self.notice(NoticeLevel::Warning, "Select a workspace first; draft retained.");
            return Vec::new();
        };
        let literal = if literal.is_empty() {
            None
        } else if let Ok(value) = ConversationSearchText::new(literal.to_owned()) {
            Some(value)
        } else {
            self.notice(NoticeLevel::Warning, "Search must be 1–256 inert text bytes.");
            return Vec::new();
        };
        let Ok(query) = ConversationLibraryQuery::new(workspace, literal, true, 0, 64) else {
            return Vec::new();
        };
        self.chat.workbench.open = true;
        self.chat.workbench.library = None;
        self.request(
            AppRequestPayload::QueryConversationLibrary(query.clone()),
            PendingRequest::ConversationLibrary(query),
        )
        .into_iter()
        .collect()
    }
}
