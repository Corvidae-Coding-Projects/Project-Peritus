//! Checkpoint-bound conversation forks.

use crate::model::{AppModel, Effect, NoticeLevel, decode_hex_16};
use peritus_app_protocol::{
    ControlOperationId, ConversationId, ConversationTitle, WorkbenchForkMode, WorkbenchForkRequest,
    WorkbenchIntent, WorkbenchQuery,
};

impl AppModel {
    pub(in crate::model::chat) fn fork_command(&mut self, arguments: &str) -> Vec<Effect> {
        if !self.workbench_available() {
            self.notice(
                NoticeLevel::Warning,
                "Conversation forks unavailable/offline; draft retained.",
            );
            return Vec::new();
        }
        let Some(source) = self.chat.workbench.snapshot.as_ref() else {
            self.notice(NoticeLevel::Warning, "Open the source conversation before /fork.");
            return Vec::new();
        };
        let parts: Vec<_> = arguments.split_whitespace().collect();
        let Some(checkpoint) = parts
            .first()
            .and_then(|value| decode_hex_16(value))
            .and_then(|bytes| ControlOperationId::new(bytes).ok())
        else {
            self.notice(NoticeLevel::Warning, "Use /fork <checkpoint-id> read-only, or /fork <checkpoint-id> isolated <workspace-id>.");
            return Vec::new();
        };
        let Some(references) = self
            .chat
            .workbench
            .checkpoint_receipt
            .as_ref()
            .filter(|receipt| {
                receipt.checkpoint() == checkpoint && receipt.query() == source.query()
            })
            .map(peritus_app_protocol::WorkbenchCheckpointReceipt::references)
        else {
            self.notice(
                NoticeLevel::Warning,
                "Inspect this exact checkpoint with /checkpoint show <checkpoint-id> before /fork; draft retained.",
            );
            return Vec::new();
        };
        let (mode, workspace) = match parts.get(1).copied() {
            Some("read-only") if parts.len() == 2 => {
                (WorkbenchForkMode::ReadOnlyCurrentWorkspace, source.query().workspace())
            }
            Some("isolated") if parts.len() == 3 => {
                let Some(workspace) = decode_hex_16(parts[2])
                    .and_then(|bytes| peritus_types::WorkspaceId::new(bytes).ok())
                else {
                    self.notice(
                        NoticeLevel::Warning,
                        "Isolated fork workspace ID must be 32 hexadecimal characters.",
                    );
                    return Vec::new();
                };
                (WorkbenchForkMode::IsolatedWritableWorkspace, workspace)
            }
            _ => {
                self.notice(
                    NoticeLevel::Warning,
                    "Use /fork <checkpoint-id> read-only, or isolated <workspace-id>.",
                );
                return Vec::new();
            }
        };
        let Ok(child) = ConversationId::new(self.ids.bytes(b"workbench-fork-conversation")) else {
            return Vec::new();
        };
        let Ok(title) = ConversationTitle::new(fork_title(source.title())) else {
            return Vec::new();
        };
        let Ok(request) = WorkbenchForkRequest::new(
            WorkbenchQuery::new(child, workspace),
            title,
            checkpoint,
            references.source_conversation_revision(),
            references.context_generation(),
            references.brief_revision(),
            references.goal_revision().unwrap_or(0),
            mode,
        ) else {
            return Vec::new();
        };
        self.submit_workbench(
            WorkbenchIntent::ForkConversation(request),
            source.query().workspace(),
        )
    }
}

fn fork_title(source: &ConversationTitle) -> String {
    let mut title = format!("Fork of {}", source.as_str());
    if title.len() > peritus_app_protocol::MAX_CONVERSATION_TITLE_BYTES {
        let end = title.floor_char_boundary(
            peritus_app_protocol::MAX_CONVERSATION_TITLE_BYTES - '…'.len_utf8(),
        );
        title.truncate(end);
        title.push('…');
    }
    title
}
