//! User-confirmed brief fixtures cover every field and all eligible latest-source lifecycles.

use super::{
    FixtureClass, GeneratedFixtureCase,
    values::{context, encoded, id, request},
};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ControlOperationId, ConversationId,
    WorkbenchBrief, WorkbenchBriefEntry, WorkbenchBriefField as F, WorkbenchCommand,
    WorkbenchInputId, WorkbenchInputOrder, WorkbenchInputRow, WorkbenchInputSelection,
    WorkbenchInputState as S, WorkbenchInputText, WorkbenchIntent, WorkbenchQuery,
};
use peritus_codec::{CodecError, CodecLimits};

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let scope =
        WorkbenchQuery::new(id(41, ConversationId::new), id(32, peritus_types::WorkspaceId::new));
    let mut cases = vec![encoded(
        "minimal-workbench-brief-query",
        FixtureClass::Minimal,
        &request(AppRequestPayload::QueryWorkbenchBrief(scope)),
        limits,
    )?];
    let mut entries = Vec::new();
    for (index, (name, field, state, text)) in [
        (
            "realistic-workbench-brief-objective",
            F::Objective,
            S::Queued,
            "Build an offline task board.",
        ),
        (
            "realistic-workbench-brief-acceptance",
            F::Acceptance,
            S::Held,
            "Confirm persistence after restart.",
        ),
        (
            "realistic-workbench-brief-constraints",
            F::Constraints,
            S::Incorporated,
            "No network access or uploads.",
        ),
        (
            "realistic-workbench-brief-assumptions",
            F::Assumptions,
            S::Withdrawn,
            "The user has confirmed keyboard-only use.",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let byte = 51 + u8::try_from(index).expect("four fields");
        let text = WorkbenchInputText::new(text.to_owned()).expect("text");
        let intent = WorkbenchIntent::SetBrief { field, text: text.clone() };
        let command = WorkbenchCommand::new(id(byte, ControlOperationId::new), scope, 7, intent);
        cases.push(encoded(
            name,
            FixtureClass::Realistic,
            &request(AppRequestPayload::WorkbenchCommand(command)),
            limits,
        )?);
        let selected =
            WorkbenchInputSelection::new(id(byte, WorkbenchInputId::new), 2).expect("source");
        let row = WorkbenchInputRow::new(
            selected,
            text,
            state,
            WorkbenchInputOrder::new(Vec::new()).expect("order"),
        )
        .expect("row");
        entries.push(WorkbenchBriefEntry::new(field, row).expect("entry"));
    }
    let brief = WorkbenchBrief::new(scope, 8, entries).expect("brief");
    let response = AppResponseEnvelope::new(
        context(),
        id(10, crate::RequestId::new),
        id(11, crate::CorrelationId::new),
        AppResponsePayload::WorkbenchBrief(brief),
    );
    cases.push(encoded("realistic-workbench-brief", FixtureClass::Realistic, &response, limits)?);
    Ok(cases)
}
