//! Conversation-library and exact fork compatibility fixtures.

use super::{
    FixtureClass, GeneratedFixtureCase,
    values::{context, encoded, id, request},
};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ControlOperationId, ConversationId,
    ConversationLibraryItem, ConversationLibraryPage, ConversationLibraryQuery,
    ConversationMessageSource, ConversationSearchSnippet, ConversationSearchText,
    ConversationTitle, WorkbenchBranchLineage, WorkbenchCommand, WorkbenchForkBudget,
    WorkbenchForkMode, WorkbenchForkRequest, WorkbenchIntent, WorkbenchQuery,
};
use peritus_codec::{CodecError, CodecLimits};

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let source =
        WorkbenchQuery::new(id(141, ConversationId::new), id(142, peritus_types::WorkspaceId::new));
    let child =
        WorkbenchQuery::new(id(143, ConversationId::new), id(144, peritus_types::WorkspaceId::new));
    let checkpoint = id(145, ControlOperationId::new);
    let allocation = WorkbenchForkBudget::new(60_000, 3, 8, 20_000).expect("allocation");
    let query = ConversationLibraryQuery::new(
        source.workspace(),
        Some(ConversationSearchText::new("older public requirement".to_owned()).expect("search")),
        true,
        0,
        32,
    )
    .expect("query");
    let fork = WorkbenchCommand::new(
        id(146, ControlOperationId::new),
        source,
        17,
        WorkbenchIntent::ForkConversation(
            WorkbenchForkRequest::new(
                child,
                ConversationTitle::new("Isolated checkpoint child".to_owned()).expect("title"),
                checkpoint,
                17,
                9,
                4,
                2,
                WorkbenchForkMode::IsolatedWritableWorkspace,
                Some(allocation),
            )
            .expect("fork"),
        ),
    );
    let lineage = WorkbenchBranchLineage::new(
        source,
        checkpoint,
        17,
        9,
        4,
        2,
        WorkbenchForkMode::IsolatedWritableWorkspace,
        Some(allocation),
    );
    let snippet = ConversationSearchSnippet::new(
        ConversationMessageSource::Input {
            conversation: source.conversation(),
            input: id(147, crate::WorkbenchInputId::new),
            revision: 1,
        },
        "Exact older public requirement source".to_owned(),
    )
    .expect("snippet");
    let item = ConversationLibraryItem::new(
        child,
        ConversationTitle::new("Isolated checkpoint child".to_owned()).expect("title"),
        true,
        false,
        21,
        None,
        Some(crate::WorkbenchGoalState::Paused),
        true,
        "Paused at an explicit safe boundary.".to_owned(),
        Some(snippet),
        Some(lineage),
    )
    .expect("item");
    let page = ConversationLibraryPage::new(query.clone(), 1, None, vec![item]).expect("page");
    let response = AppResponseEnvelope::new(
        context(),
        id(148, crate::RequestId::new),
        id(149, crate::CorrelationId::new),
        AppResponsePayload::ConversationLibrary(page),
    );
    Ok(vec![
        encoded(
            "minimal-conversation-library-query",
            FixtureClass::Minimal,
            &request(AppRequestPayload::QueryConversationLibrary(query)),
            limits,
        )?,
        encoded(
            "realistic-workbench-fork",
            FixtureClass::Realistic,
            &request(AppRequestPayload::WorkbenchCommand(fork)),
            limits,
        )?,
        encoded("realistic-conversation-library", FixtureClass::Realistic, &response, limits)?,
    ])
}
