//! Refresh metadata-bound commands before creating an operation from a stale panel.

use super::{AppModel, Effect};
use crate::model::chat::catalog::Command;

impl AppModel {
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
        Some(self.workbench_slash_command(command, arguments).unwrap_or_default())
    }
}
