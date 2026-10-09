//! Stable queue wire fixtures for each exact intent and the paginated public projection.

use super::{
    FixtureClass, GeneratedFixtureCase,
    values::{context, encoded, id, request},
};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ControlOperationId, ConversationId,
    WorkbenchCommand, WorkbenchInputId, WorkbenchInputOrder, WorkbenchInputRow,
    WorkbenchInputSelection, WorkbenchInputState, WorkbenchInputText, WorkbenchIntent,
    WorkbenchNewInput, WorkbenchQuery, WorkbenchQueueIntent, WorkbenchQueuePage,
    WorkbenchQueueQuery,
};
use peritus_codec::{CodecError, CodecLimits};

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let query =
        WorkbenchQuery::new(id(41, ConversationId::new), id(32, peritus_types::WorkspaceId::new));
    let operation = id(42, ControlOperationId::new);
    let input = id(44, WorkbenchInputId::new);
    let selected = WorkbenchInputSelection::new(input, 1).expect("selection");
    let text = WorkbenchInputText::new(
        "Use the corrected requirement.\n/stop is inert quoted text.".to_owned(),
    )
    .expect("text");
    let mut cases = Vec::new();
    for (name, intent) in [
        (
            "realistic-workbench-enqueue",
            WorkbenchQueueIntent::Enqueue(
                WorkbenchNewInput::new(
                    input,
                    text.clone(),
                    WorkbenchInputOrder::new(Vec::new()).expect("order"),
                )
                .expect("input"),
            ),
        ),
        (
            "realistic-workbench-edit-input",
            WorkbenchQueueIntent::Edit { selected, text: text.clone() },
        ),
        (
            "realistic-workbench-correct-input",
            WorkbenchQueueIntent::Correct {
                original: selected,
                id: id(45, WorkbenchInputId::new),
                text: text.clone(),
            },
        ),
        ("minimal-workbench-hold-input", WorkbenchQueueIntent::Hold { selected, held: true }),
        ("minimal-workbench-withdraw-input", WorkbenchQueueIntent::Withdraw(selected)),
        (
            "minimal-workbench-order-inputs",
            WorkbenchQueueIntent::Reorder(WorkbenchInputOrder::new(vec![input]).expect("order")),
        ),
    ] {
        let command = WorkbenchCommand::new(operation, query, 8, WorkbenchIntent::Queue(intent));
        cases.push(encoded(
            name,
            FixtureClass::Realistic,
            &request(AppRequestPayload::WorkbenchCommand(command)),
            limits,
        )?);
    }
    let page_query = WorkbenchQueueQuery::new(query, 9, 0, true).expect("query");
    cases.push(encoded(
        "minimal-workbench-queue-query",
        FixtureClass::Minimal,
        &request(AppRequestPayload::QueryWorkbenchQueue(page_query)),
        limits,
    )?);
    let rows = vec![
        WorkbenchInputRow::new(
            selected,
            text,
            WorkbenchInputState::Superseded,
            WorkbenchInputOrder::new(Vec::new()).expect("order"),
        )
        .expect("row"),
    ];
    let page = WorkbenchQueuePage::new(page_query, 1, rows).expect("page");
    let response = AppResponseEnvelope::new(
        context(),
        id(10, crate::RequestId::new),
        id(11, crate::CorrelationId::new),
        AppResponsePayload::WorkbenchQueue(page),
    );
    cases.push(encoded(
        "realistic-workbench-queue-page",
        FixtureClass::Realistic,
        &response,
        limits,
    )?);
    Ok(cases)
}
