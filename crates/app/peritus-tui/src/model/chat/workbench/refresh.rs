//! Refresh metadata-bound commands before creating an operation from a stale panel.

use super::{AppModel, Effect, NoticeLevel};
use crate::model::chat::catalog::Command;

impl AppModel {
    pub(super) fn create_command_conversation(
        &mut self,
        title: &str,
        command: String,
    ) -> Vec<Effect> {
        if self.workbench_request_pending() || self.chat.workbench.unresolved.is_some() {
            self.notice(
                NoticeLevel::Warning,
                "Resolve the pending request before opening a conversation; draft retained.",
            );
            return Vec::new();
        }
        let Some(workspace) = self.product.as_ref().map(|product| product.launch.workspace_id())
        else {
            return Vec::new();
        };
        let Some((query, revision)) = self.new_conversation_binding(workspace) else {
            return Vec::new();
        };
        let title =
            peritus_app_protocol::ConversationTitle::new(title.to_owned()).expect("static title");
        self.select_workbench_conversation(Some(query));
        let effects = self.submit_bound_workbench(
            peritus_app_protocol::WorkbenchIntent::CreateConversation(title),
            query,
            revision,
        );
        if effects.is_empty() {
            self.select_workbench_conversation(None);
            return effects;
        }
        // Continue only after the exact creation receipt and a fresh snapshot. The shared
        // continuation guard abandons the read when Esc, a new selection, or draft edits intervene.
        self.chat.workbench.snapshot_refresh_command =
            Some((query, self.chat.buffer.clone(), command));
        "Creating a conversation for this command. No inference started."
            .clone_into(&mut self.chat.workbench.message);
        effects
    }

    pub(in crate::model::chat) fn refresh_stale_command_snapshot(
        &mut self,
        command: Command,
        arguments: &str,
    ) -> Option<Vec<Effect>> {
        let uses_snapshot =
            matches!(command, Command::Checkpoint | Command::Rewind | Command::Fork)
                || (command == Command::Sessions
                    && matches!(
                        arguments.split_whitespace().next(),
                        Some("rename" | "pin" | "unpin" | "archive" | "unarchive")
                    ));
        if !uses_snapshot
            || !self.workbench_available()
            || self.workbench_request_pending()
            || self.chat.workbench.unresolved.is_some()
        {
            return None;
        }
        let query = self.chat.workbench.selected?;
        // Owned mutations also create automatic checkpoints without a UI receipt.
        // Even a snapshot matching our last receipt can therefore be stale.
        Some(self.refresh_snapshot_for_command(query, self.chat.buffer.clone()))
    }

    pub(super) fn refresh_snapshot_for_command(
        &mut self,
        query: peritus_app_protocol::WorkbenchQuery,
        command: String,
    ) -> Vec<Effect> {
        self.chat.workbench.snapshot_refresh_command =
            Some((query, self.chat.buffer.clone(), command));
        self.refresh_selected_snapshot()
    }

    pub(in crate::model) fn complete_command_snapshot_refresh(&mut self) -> Option<Vec<Effect>> {
        let (query, draft, command) = self.chat.workbench.snapshot_refresh_command.take()?;
        self.chat.workbench.inspection_draft = None;
        if self.chat.workbench.selected != Some(query) || self.chat.buffer != draft {
            return Some(Vec::new());
        }
        let Ok((command, arguments)) = crate::model::chat::catalog::parse(&command) else {
            return Some(Vec::new());
        };
        self.chat.workbench.message.clear();
        let effects = self.workbench_slash_command(command, arguments).unwrap_or_default();
        self.track_workbench_inspection(&effects);
        Some(effects)
    }
}
