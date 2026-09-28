//! Conversation-library navigation and metadata commands.

use super::WorkbenchMode;
use crate::model::{AppModel, Effect, NoticeLevel, PendingRequest, decode_hex_16};
use peritus_app_protocol::{
    AppRequestPayload, ConversationId, ConversationLibraryQuery, ConversationSearchText,
    ConversationTitle, WorkbenchIntent, WorkbenchQuery,
};

impl AppModel {
    fn open_legacy_library_conversation(
        &mut self,
        id: ConversationId,
        workspace: peritus_types::WorkspaceId,
    ) -> Option<Vec<Effect>> {
        let run = self
            .chat
            .workbench
            .library
            .as_ref()
            .and_then(|page| {
                page.items().iter().find(|item| item.query() == WorkbenchQuery::new(id, workspace))
            })
            .and_then(peritus_app_protocol::ConversationLibraryItem::legacy_run)?;
        self.select_workbench_conversation(None);
        self.chat.workbench.open = false;
        self.chat.run_id = Some(run);
        self.chat.snapshot = None;
        self.clear_chat_command();
        Some(
            self.request(
                AppRequestPayload::QueryInteraction(
                    peritus_app_protocol::ProductRunConversationQuery::new(run),
                ),
                PendingRequest::ChatOpen { run_id: run },
            )
            .into_iter()
            .collect(),
        )
    }

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
            let parts: Vec<_> = text.split_whitespace().collect();
            let Some((id, target)) = parse_open_target(&parts, workspace) else {
                self.notice(
                    NoticeLevel::Warning,
                    "Use /sessions open <conversation-id> [workspace-id], each 32 hexadecimal characters; draft retained.",
                );
                return Vec::new();
            };
            if target != workspace {
                self.chat.workbench.message = format!("Opening /sessions open {text}");
                return vec![Effect::OpenConversation(WorkbenchQuery::new(id, target))];
            }
            if let Some(effects) = self.open_legacy_library_conversation(id, workspace) {
                return effects;
            }
            self.select_workbench_conversation(Some(WorkbenchQuery::new(id, workspace)));
            self.chat.workbench.mode = WorkbenchMode::Sessions;
            self.chat.workbench.open = true;
            return self.discover_workbench_execution();
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
        self.chat.workbench.mode = WorkbenchMode::Library;
        self.chat.workbench.library_selected = 0;
        self.chat.workbench.scroll = 0;
        self.chat.workbench.message = "Loading conversations…".into();
        self.chat.workbench.library = None;
        self.chat.workbench.library_query = Some(query.clone());
        self.request(
            AppRequestPayload::QueryConversationLibrary(query.clone()),
            PendingRequest::ConversationLibrary(query),
        )
        .into_iter()
        .collect()
    }
}

fn parse_open_target(
    parts: &[&str],
    current: peritus_types::WorkspaceId,
) -> Option<(ConversationId, peritus_types::WorkspaceId)> {
    if !(1..=2).contains(&parts.len()) {
        return None;
    }
    let conversation = ConversationId::new(decode_hex_16(parts[0])?).ok()?;
    let workspace = match parts.get(1) {
        Some(value) => peritus_types::WorkspaceId::new(decode_hex_16(value)?).ok()?,
        None => current,
    };
    Some((conversation, workspace))
}
