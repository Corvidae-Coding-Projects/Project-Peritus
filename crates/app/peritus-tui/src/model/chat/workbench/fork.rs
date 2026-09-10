//! Checkpoint-bound conversation forks and isolated allocation parsing.

use crate::model::{AppModel, Effect, NoticeLevel, decode_hex_16};
use peritus_app_protocol::{
    ControlOperationId, ConversationId, ConversationTitle, WorkbenchForkBudget, WorkbenchForkMode,
    WorkbenchForkRequest, WorkbenchIntent, WorkbenchQuery,
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
            self.notice(NoticeLevel::Warning, "Use /fork <checkpoint-id> read-only [time=<ms> requests=<n> tools=<n> tokens=<n>], or /fork <checkpoint-id> isolated <workspace-id> time=<ms> requests=<n> tools=<n> tokens=<n>.");
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
        let (mode, workspace, allocation) = match parts.get(1).copied() {
            Some("read-only") if parts.len() == 2 || parts.len() == 6 => (
                WorkbenchForkMode::ReadOnlyCurrentWorkspace,
                source.query().workspace(),
                if parts.len() == 2 { None } else { parse_fork_budget(&parts[2..]) },
            ),
            Some("isolated") if parts.len() == 7 => {
                let Some(workspace) = decode_hex_16(parts[2])
                    .and_then(|bytes| peritus_types::WorkspaceId::new(bytes).ok())
                else {
                    self.notice(
                        NoticeLevel::Warning,
                        "Isolated fork workspace ID must be 32 hexadecimal characters.",
                    );
                    return Vec::new();
                };
                let Some(allocation) = parse_fork_budget(&parts[3..]) else {
                    self.notice(
                        NoticeLevel::Warning,
                        "Isolated allocation requires time=<ms> requests=<n> tools=<n> tokens=<n>.",
                    );
                    return Vec::new();
                };
                (WorkbenchForkMode::IsolatedWritableWorkspace, workspace, Some(allocation))
            }
            _ => {
                self.notice(NoticeLevel::Warning, "Use /fork <checkpoint-id> read-only with zero or all four allocation fields, or isolated <workspace-id> with all four allocation fields.");
                return Vec::new();
            }
        };
        if let Err(message) =
            validate_fork_allocation(references.goal_revision(), parts.len(), allocation)
        {
            self.notice(NoticeLevel::Warning, message);
            return Vec::new();
        }
        let Ok(child) = ConversationId::new(self.ids.bytes(b"workbench-fork-conversation")) else {
            return Vec::new();
        };
        let title = format!("Fork of {}", source.title().as_str());
        let Ok(title) = ConversationTitle::new(title) else { return Vec::new() };
        let Ok(request) = WorkbenchForkRequest::new(
            WorkbenchQuery::new(child, workspace),
            title,
            checkpoint,
            references.source_conversation_revision(),
            references.context_generation(),
            references.brief_revision(),
            references.goal_revision().unwrap_or(0),
            mode,
            allocation,
        ) else {
            return Vec::new();
        };
        self.submit_workbench(
            WorkbenchIntent::ForkConversation(request),
            source.query().workspace(),
        )
    }
}

const fn validate_fork_allocation(
    goal_revision: Option<u64>,
    argument_count: usize,
    allocation: Option<WorkbenchForkBudget>,
) -> Result<(), &'static str> {
    if argument_count == 6 && allocation.is_none() {
        return Err("Read-only allocation requires time=<ms> requests=<n> tools=<n> tokens=<n>.");
    }
    match (goal_revision, allocation) {
        (Some(_), None) => Err("A governed checkpoint fork requires all four allocation fields."),
        (None, Some(_)) => Err("An ungoverned checkpoint fork cannot reserve a goal allocation."),
        _ => Ok(()),
    }
}

pub(super) fn parse_fork_budget(parts: &[&str]) -> Option<WorkbenchForkBudget> {
    let mut time = None;
    let mut requests = None;
    let mut tools = None;
    let mut tokens = None;
    for part in parts {
        let (key, value) = part.split_once('=')?;
        match key {
            "time" if time.is_none() => time = value.parse().ok(),
            "requests" if requests.is_none() => requests = value.parse().ok(),
            "tools" if tools.is_none() => tools = value.parse().ok(),
            "tokens" if tokens.is_none() => tokens = value.parse().ok(),
            _ => return None,
        }
    }
    WorkbenchForkBudget::new(time?, requests?, tools?, tokens?).ok()
}
