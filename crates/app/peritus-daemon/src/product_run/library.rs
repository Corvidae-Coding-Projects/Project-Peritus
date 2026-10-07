//! Durable local conversation-library projection and literal public-message search.

use super::ProductRunService;
use crate::product_control::{ControlStore, ControlStoreError};
use peritus_app_protocol::{
    AppResponsePayload, ControlOperationId, ConversationLibraryItem, ConversationLibraryPage,
    ConversationLibraryQuery, ConversationMessageSource, ConversationSearchSnippet,
    ConversationTitle, WorkbenchBranchLineage, WorkbenchForkMode, WorkbenchGoalState,
    WorkbenchQuery,
};
use peritus_product_runner::control::{
    ConversationBranch, ConversationBranchMode, ConversationRecord, GoalState,
};
use peritus_types::{ActorId, WorkspaceId};

struct GovernedItem {
    record: ConversationRecord,
    activity: u64,
    snippet: Option<ConversationSearchSnippet>,
    branch: Option<ConversationBranch>,
}

impl ProductRunService {
    pub(crate) fn conversation_library(
        &self,
        actor: ActorId,
        query: &ConversationLibraryQuery,
    ) -> AppResponsePayload {
        if !self.inner.workspaces.contains_key(&query.workspace()) {
            return super::ProductRunServiceError::WorkspaceUnavailable.response();
        }
        let mut items = match self.governed_library_items(actor, query) {
            Ok(items) => items,
            Err(error) => return super::ProductRunServiceError::from(error).response(),
        };
        items.sort_by(|left, right| {
            right
                .pinned()
                .cmp(&left.pinned())
                .then_with(|| right.activity_revision().cmp(&left.activity_revision()))
                .then_with(|| {
                    left.query()
                        .conversation()
                        .as_bytes()
                        .cmp(right.query().conversation().as_bytes())
                })
        });
        let total = match u32::try_from(items.len()) {
            Ok(value) => value,
            Err(_) => return super::ProductRunServiceError::Unavailable.response(),
        };
        let start = usize::try_from(query.offset()).unwrap_or(usize::MAX).min(items.len());
        let end = start.saturating_add(usize::from(query.limit())).min(items.len());
        let page_items: Vec<_> = items.drain(start..end).collect();
        let next = (end < items.len().saturating_add(page_items.len()))
            .then(|| u32::try_from(end).ok())
            .flatten();
        ConversationLibraryPage::new(query.clone(), total, next, page_items).map_or_else(
            |_| super::ProductRunServiceError::InvalidMessage.response(),
            AppResponsePayload::ConversationLibrary,
        )
    }

    fn governed_library_items(
        &self,
        actor: ActorId,
        query: &ConversationLibraryQuery,
    ) -> Result<Vec<ConversationLibraryItem>, ControlStoreError> {
        self.with_control_index_read(|store| {
            let mut items = Vec::new();
            for (id, activity) in store.conversation_ids()? {
                let record = store
                    .load(id)?
                    .ok_or(ControlStoreError::Corrupt("indexed conversation missing"))?;
                if record.owner_bytes() != actor.as_bytes()
                    || record.workspace_bytes() != query.workspace().as_bytes()
                    || (!query.include_archived() && record.archived())
                {
                    continue;
                }
                let snippet = governed_snippet(
                    store,
                    &record,
                    query.literal().map(peritus_app_protocol::ConversationSearchText::as_str),
                )?;
                if query.literal().is_some_and(|literal| {
                    !record.title().contains(literal.as_str()) && snippet.is_none()
                }) {
                    continue;
                }
                let branch = store.branch(id)?;
                items.push(project_governed(record, activity, snippet, branch)?);
            }
            Ok(items)
        })
        .or_else(|error| {
            if matches!(
                error,
                ControlStoreError::Control(peritus_product_runner::control::ControlError::NotFound)
            ) {
                Ok(Vec::new())
            } else {
                Err(error)
            }
        })
    }
}

fn project_governed(
    value: ConversationRecord,
    activity: u64,
    snippet: Option<ConversationSearchSnippet>,
    branch: Option<ConversationBranch>,
) -> Result<ConversationLibraryItem, ControlStoreError> {
    let workspace = WorkspaceId::new(*value.workspace_bytes())
        .map_err(|_| ControlStoreError::Corrupt("invalid governed workspace"))?;
    let conversation = peritus_app_protocol::ConversationId::new(*value.id().as_bytes())
        .map_err(|_| ControlStoreError::Corrupt("invalid governed conversation"))?;
    let query = WorkbenchQuery::new(conversation, workspace);
    let title = ConversationTitle::new(value.title().to_owned())
        .map_err(|_| ControlStoreError::Corrupt("invalid governed title"))?;
    let goal_state = value.goal().map(|goal| public_goal_state(goal.state()));
    let handoff = value.goal().map_or_else(String::new, |goal| goal.reason().to_owned());
    let lineage = branch.as_ref().map(public_lineage).transpose()?;
    ConversationLibraryItem::new(
        query,
        title,
        value.pinned(),
        value.archived(),
        activity,
        goal_state,
        branch.as_ref().is_some_and(|branch| branch.objective().is_some())
            && value.execution().is_none(),
        bounded_handoff(&handoff),
        snippet,
        lineage,
    )
    .map_err(|_| ControlStoreError::Corrupt("invalid governed library projection"))
}

fn governed_snippet(
    store: &ControlStore,
    record: &ConversationRecord,
    literal: Option<&str>,
) -> Result<Option<ConversationSearchSnippet>, ControlStoreError> {
    let Some(literal) = literal else { return Ok(None) };
    for input in record.inputs().revisions() {
        if input.text().contains(literal) {
            let source = ConversationMessageSource::Input {
                conversation: peritus_app_protocol::ConversationId::new(*record.id().as_bytes())
                    .map_err(|_| ControlStoreError::Corrupt("invalid input conversation"))?,
                input: peritus_app_protocol::WorkbenchInputId::new(
                    *input.selection().id().as_bytes(),
                )
                .map_err(|_| ControlStoreError::Corrupt("invalid input identity"))?,
                revision: input.selection().revision(),
            };
            return ConversationSearchSnippet::new(source, excerpt(input.text(), literal))
                .map(Some)
                .map_err(|_| ControlStoreError::Corrupt("invalid input snippet"));
        }
    }
    for reply in record.replies() {
        let text = store.reply_text(reply)?;
        if text.contains(literal) {
            let source = ConversationMessageSource::Reply {
                conversation: peritus_app_protocol::ConversationId::new(*record.id().as_bytes())
                    .map_err(|_| ControlStoreError::Corrupt("invalid reply conversation"))?,
                operation: ControlOperationId::new(*reply.operation().as_bytes())
                    .map_err(|_| ControlStoreError::Corrupt("invalid reply operation"))?,
            };
            return ConversationSearchSnippet::new(source, excerpt(&text, literal))
                .map(Some)
                .map_err(|_| ControlStoreError::Corrupt("invalid reply snippet"));
        }
    }
    Ok(None)
}

fn excerpt(text: &str, literal: &str) -> String {
    let match_start = text.find(literal).unwrap_or(0);
    let mut start = match_start.saturating_sub(192);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    let mut end = match_start.saturating_add(literal.len()).saturating_add(192).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[start..end].to_owned()
}

fn public_lineage(value: &ConversationBranch) -> Result<WorkbenchBranchLineage, ControlStoreError> {
    let parent = WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new(*value.source().as_bytes())
            .map_err(|_| ControlStoreError::Corrupt("invalid branch parent"))?,
        WorkspaceId::new(*value.source_workspace_bytes())
            .map_err(|_| ControlStoreError::Corrupt("invalid branch source workspace"))?,
    );
    let checkpoint = ControlOperationId::new(*value.checkpoint().as_bytes())
        .map_err(|_| ControlStoreError::Corrupt("invalid branch checkpoint"))?;
    let mode = match value.mode() {
        ConversationBranchMode::ReadOnlyCurrentWorkspace => {
            WorkbenchForkMode::ReadOnlyCurrentWorkspace
        }
        ConversationBranchMode::IsolatedWritableWorkspace => {
            WorkbenchForkMode::IsolatedWritableWorkspace
        }
    };
    Ok(WorkbenchBranchLineage::new(
        parent,
        checkpoint,
        value.source_revision(),
        value.context_generation(),
        value.brief_revision(),
        value.goal_revision(),
        mode,
    ))
}

const fn public_goal_state(value: GoalState) -> WorkbenchGoalState {
    match value {
        GoalState::Active => WorkbenchGoalState::Active,
        GoalState::WaitingForUser => WorkbenchGoalState::WaitingForUser,
        GoalState::Pausing => WorkbenchGoalState::Pausing,
        GoalState::Paused => WorkbenchGoalState::Paused,
        GoalState::Blocked => WorkbenchGoalState::Blocked,
        GoalState::Achieved => WorkbenchGoalState::Achieved,
        GoalState::Cancelled => WorkbenchGoalState::Cancelled,
    }
}

fn bounded_handoff(value: &str) -> String {
    bounded(value, 1024)
}

fn bounded(value: &str, maximum: usize) -> String {
    // Library rows are single-line labels, while task text and summaries may be multiline.
    // Normalize only this presentation; immutable messages and exact search snippets stay intact.
    let value: String = value.chars().map(|ch| if ch.is_control() { ' ' } else { ch }).collect();
    if value.len() <= maximum {
        return value;
    }
    let mut end = maximum;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiline_task_and_handoff_project_as_bounded_library_labels() {
        let handoff = bounded_handoff(&format!("Done\n{}\r\n", "λ".repeat(1024)));
        assert!(handoff.len() <= 1024);
        assert!(!handoff.chars().any(char::is_control));
    }
}
