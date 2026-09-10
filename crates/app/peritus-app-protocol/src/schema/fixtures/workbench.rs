//! Exact additive control frames covering every initial intent and projection.

use super::{
    FixtureClass, GeneratedFixtureCase,
    values::{context, encoded, id, request},
};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ControlOperationId, ConversationId,
    ConversationTitle, WorkbenchCommand, WorkbenchIntent, WorkbenchQuery, WorkbenchReceipt,
    WorkbenchSnapshot,
};
use peritus_codec::{CodecError, CodecLimits};

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let query =
        WorkbenchQuery::new(id(41, ConversationId::new), id(32, peritus_types::WorkspaceId::new));
    let operation = id(42, ControlOperationId::new);
    let title = ConversationTitle::new("Tetris workbench".to_owned()).expect("title");
    let create = WorkbenchCommand::new(
        operation,
        query,
        0,
        WorkbenchIntent::CreateConversation(title.clone()),
    );
    let mut cases = Vec::new();
    for (name, command) in [
        ("minimal-workbench-create", create.clone()),
        (
            "realistic-workbench-rename",
            WorkbenchCommand::new(
                operation,
                query,
                1,
                WorkbenchIntent::RenameConversation(title.clone()),
            ),
        ),
        (
            "minimal-workbench-pin",
            WorkbenchCommand::new(operation, query, 1, WorkbenchIntent::PinConversation(true)),
        ),
        (
            "minimal-workbench-archive",
            WorkbenchCommand::new(operation, query, 1, WorkbenchIntent::ArchiveConversation(false)),
        ),
        (
            "minimal-workbench-start",
            WorkbenchCommand::new(
                operation,
                query,
                2,
                WorkbenchIntent::StartExecution(crate::WorkbenchExecutionSettings::new(
                    id(31, peritus_types::RunId::new),
                    crate::ProductProviderSelection::new(
                        id(33, peritus_types::ProviderProfileId::new),
                        id(33, peritus_types::ProviderProfileId::new),
                        id(33, peritus_types::ProviderProfileId::new),
                    ),
                    crate::ProductInteractionMode::Chat,
                    crate::ProductRoleModels::default(),
                )),
            ),
        ),
    ] {
        cases.push(encoded(
            name,
            FixtureClass::Minimal,
            &request(AppRequestPayload::WorkbenchCommand(command)),
            limits,
        )?);
    }
    cases.push(encoded(
        "minimal-workbench-query",
        FixtureClass::Minimal,
        &request(AppRequestPayload::QueryWorkbench(query)),
        limits,
    )?);
    cases.push(encoded(
        "minimal-workbench-receipt-query",
        FixtureClass::Minimal,
        &request(AppRequestPayload::QueryWorkbenchReceipt(create)),
        limits,
    )?);
    for (name, payload) in [
        (
            "realistic-workbench-snapshot",
            AppResponsePayload::Workbench(
                WorkbenchSnapshot::new(query, 7, title, true, false).expect("snapshot"),
            ),
        ),
        (
            "realistic-workbench-receipt",
            AppResponsePayload::WorkbenchReceipt(
                WorkbenchReceipt::new(
                    operation,
                    query,
                    1,
                    peritus_types::Sha256Digest::new([43; 32]),
                )
                .expect("receipt"),
            ),
        ),
    ] {
        let response = AppResponseEnvelope::new(
            context(),
            id(10, crate::RequestId::new),
            id(11, crate::CorrelationId::new),
            payload,
        );
        cases.push(encoded(name, FixtureClass::Realistic, &response, limits)?);
    }
    Ok(cases)
}
