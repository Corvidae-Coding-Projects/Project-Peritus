//! Explicit project-guidance query, mutation, and lifecycle compatibility frames.

use super::{
    FixtureClass, GeneratedFixtureCase,
    values::{context, encoded, id, request},
};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ControlOperationId, ConversationId,
    WorkbenchCommand, WorkbenchGuidanceContent, WorkbenchGuidanceForget, WorkbenchGuidancePin,
    WorkbenchGuidanceReason, WorkbenchGuidanceRecord, WorkbenchGuidanceRevision,
    WorkbenchGuidanceSave, WorkbenchGuidanceScope, WorkbenchGuidanceScopeChange,
    WorkbenchGuidanceSelection, WorkbenchGuidanceSource, WorkbenchGuidanceText, WorkbenchIntent,
    WorkbenchInvocationId, WorkbenchMemory, WorkbenchMemoryQuery, WorkbenchMemoryRow,
    WorkbenchQuery,
};
use peritus_codec::{CodecError, CodecLimits};

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let query =
        WorkbenchQuery::new(id(131, ConversationId::new), id(132, peritus_types::WorkspaceId::new));
    let memory_query = WorkbenchMemoryQuery::new(query, 6, 0, true).expect("memory query");
    let mut cases = vec![encoded(
        "minimal-workbench-memory-query",
        FixtureClass::Minimal,
        &request(AppRequestPayload::QueryWorkbenchMemory(memory_query)),
        limits,
    )?];

    cases.extend(mutation_cases(query, limits)?);
    cases.push(memory_case(query, memory_query, limits)?);
    Ok(cases)
}

fn mutation_cases(
    query: WorkbenchQuery,
    limits: CodecLimits,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let saved_identity = id(133, ControlOperationId::new);
    let selected = WorkbenchGuidanceSelection::new(saved_identity, 1).expect("selection");
    let accepted_text = WorkbenchGuidanceText::new(
        "Run the focused protocol tests before the workspace-wide gate.".to_owned(),
    )
    .expect("accepted text");
    let accepted_content = WorkbenchGuidanceContent::new(
        accepted_text.clone(),
        WorkbenchGuidanceSource::AcceptedPublicReply {
            operation: id(134, ControlOperationId::new),
            invocation: id(135, WorkbenchInvocationId::new),
            digest: peritus_codec::sha256(accepted_text.as_str().as_bytes()),
        },
        WorkbenchGuidanceScope::Project,
    )
    .expect("accepted content");
    let commands = [
        (
            "realistic-workbench-memory-save",
            id(136, ControlOperationId::new),
            WorkbenchIntent::SaveGuidance(WorkbenchGuidanceSave::new(
                6,
                content("Preserve exact user-approved guidance.", WorkbenchGuidanceScope::Project),
                false,
            )),
        ),
        (
            "realistic-workbench-memory-revise",
            id(137, ControlOperationId::new),
            WorkbenchIntent::ReviseGuidance(WorkbenchGuidanceRevision::new(
                selected,
                6,
                accepted_content,
            )),
        ),
        (
            "realistic-workbench-memory-pin",
            id(138, ControlOperationId::new),
            WorkbenchIntent::PinGuidance(WorkbenchGuidancePin::new(selected, 6, true)),
        ),
        (
            "realistic-workbench-memory-scope",
            id(139, ControlOperationId::new),
            WorkbenchIntent::ScopeGuidance(WorkbenchGuidanceScopeChange::new(
                selected,
                6,
                WorkbenchGuidanceScope::Conversation(query.conversation()),
            )),
        ),
        (
            "realistic-workbench-memory-forget",
            id(140, ControlOperationId::new),
            WorkbenchIntent::ForgetGuidance(WorkbenchGuidanceForget::new(
                selected,
                6,
                WorkbenchGuidanceReason::new("Superseded by current project policy.".to_owned())
                    .expect("reason"),
            )),
        ),
    ];
    commands
        .into_iter()
        .map(|(name, operation, intent)| {
            encoded(
                name,
                FixtureClass::Realistic,
                &request(AppRequestPayload::WorkbenchCommand(WorkbenchCommand::new(
                    operation, query, 6, intent,
                ))),
                limits,
            )
        })
        .collect()
}

fn memory_case(
    query: WorkbenchQuery,
    memory_query: WorkbenchMemoryQuery,
    limits: CodecLimits,
) -> Result<GeneratedFixtureCase, CodecError> {
    let active = WorkbenchGuidanceRecord::save(
        id(141, ControlOperationId::new),
        query.workspace(),
        WorkbenchGuidanceSave::new(
            3,
            content("Keep validation evidence reproducible.", WorkbenchGuidanceScope::Project),
            true,
        ),
    )
    .expect("active record");
    let forgotten = WorkbenchGuidanceRecord::save(
        id(142, ControlOperationId::new),
        query.workspace(),
        WorkbenchGuidanceSave::new(
            4,
            content("Use the retired one-shot workflow.", WorkbenchGuidanceScope::Project),
            false,
        ),
    )
    .expect("prior record")
    .forget(
        id(143, ControlOperationId::new),
        WorkbenchGuidanceForget::new(
            WorkbenchGuidanceSelection::new(id(142, ControlOperationId::new), 1)
                .expect("forgotten selection"),
            5,
            WorkbenchGuidanceReason::new("The workflow was retired.".to_owned())
                .expect("forgotten reason"),
        ),
    )
    .expect("tombstone");
    let memory = WorkbenchMemory::new(
        memory_query,
        6,
        2,
        vec![WorkbenchMemoryRow::Active(active), WorkbenchMemoryRow::Forgotten(forgotten)],
    )
    .expect("memory response");
    let response = AppResponseEnvelope::new(
        context(),
        id(144, crate::RequestId::new),
        id(145, crate::CorrelationId::new),
        AppResponsePayload::WorkbenchMemory(memory),
    );
    encoded("realistic-workbench-memory", FixtureClass::Realistic, &response, limits)
}

fn content(text: &str, scope: WorkbenchGuidanceScope) -> WorkbenchGuidanceContent {
    WorkbenchGuidanceContent::new(
        WorkbenchGuidanceText::new(text.to_owned()).expect("guidance text"),
        WorkbenchGuidanceSource::UserAuthored,
        scope,
    )
    .expect("guidance content")
}
