use super::*;
use peritus_app_protocol::{
    ProtocolFeatureName, WellKnownProtocolFeature, WorkbenchGuidanceLifecycle,
    WorkbenchGuidanceRecord, WorkbenchGuidanceScope, WorkbenchGuidanceSource, WorkbenchIntent,
    WorkbenchMemory, WorkbenchMemoryQuery, WorkbenchMemoryRow, WorkbenchQuery, WorkbenchQueuePage,
    WorkbenchQueueQuery,
};
use ratatui::{Terminal, backend::TestBackend};

fn memory_model() -> AppModel {
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchMemory)
            .expect("feature"),
    );
    model
}

fn open_conversation(model: &mut AppModel) -> WorkbenchCommand {
    let (created, command) = create(model);
    let query_request = request(&respond(model, &created, receipt(&command)));
    let snapshot = WorkbenchSnapshot::new(
        command.query(),
        1,
        ConversationTitle::new("Memory fixture".to_owned()).expect("title"),
        false,
        false,
    )
    .expect("snapshot");
    respond(model, &query_request, AppResponsePayload::Workbench(snapshot));
    command
}

fn memory_response(
    request: &AppRequestEnvelope,
    dependency_revision: u64,
    rows: Vec<WorkbenchMemoryRow>,
) -> AppResponsePayload {
    let AppRequestPayload::QueryWorkbenchMemory(query) = request.payload() else {
        panic!("memory query")
    };
    AppResponsePayload::WorkbenchMemory(
        WorkbenchMemory::new(
            *query,
            dependency_revision,
            u32::try_from(rows.len()).expect("bounded rows"),
            rows,
        )
        .expect("memory page"),
    )
}

fn snapshot(query: WorkbenchQuery, revision: u64) -> WorkbenchSnapshot {
    WorkbenchSnapshot::new(
        query,
        revision,
        ConversationTitle::new("Memory fixture".to_owned()).expect("title"),
        false,
        false,
    )
    .expect("snapshot")
}

fn begin_memory_inspection(
    model: &mut AppModel,
    command: &str,
    query: WorkbenchQuery,
    revision: u64,
) -> AppRequestEnvelope {
    let snapshot_request = request(&model.slash_command(command));
    assert_eq!(snapshot_request.payload(), &AppRequestPayload::QueryWorkbench(query));
    request(&respond(
        model,
        &snapshot_request,
        AppResponsePayload::Workbench(snapshot(query, revision)),
    ))
}

fn accept_and_refresh(
    model: &mut AppModel,
    sent: &AppRequestEnvelope,
    command: &WorkbenchCommand,
    dependency_revision: u64,
    rows: Vec<WorkbenchMemoryRow>,
) {
    let query = request(&respond(model, sent, receipt(command)));
    let snapshot = WorkbenchSnapshot::new(
        command.query(),
        command.expected_revision() + 1,
        ConversationTitle::new("Memory fixture".to_owned()).expect("title"),
        false,
        false,
    )
    .expect("snapshot");
    let memory = request(&respond(model, &query, AppResponsePayload::Workbench(snapshot)));
    respond(model, &memory, memory_response(&memory, dependency_revision, rows));
}

fn entered(model: &mut AppModel, text: String) -> (AppRequestEnvelope, WorkbenchCommand) {
    key(model, KeyCode::Esc);
    model.chat.buffer = text;
    let sent = request(&key(model, KeyCode::Enter));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else {
        panic!("workbench command")
    };
    (sent.clone(), command.clone())
}

fn assert_content_free_history(model: &AppModel) {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
    let frame = terminal.draw(|frame| crate::render::draw(frame, model)).expect("draw");
    let text: String = frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect();
    assert!(text.contains("Content is intentionally absent"));
    assert!(text.contains("does not erase past prompts"));
    assert!(!text.contains("Run exact focused acceptance tests."));
}

#[test]
fn memory_save_revise_pin_scope_forget_and_history_are_explicit_roundtrips() {
    let mut model = memory_model();
    let conversation = open_conversation(&mut model);

    let inspect = begin_memory_inspection(&mut model, "/memory", conversation.query(), 1);
    assert_eq!(
        inspect.payload(),
        &AppRequestPayload::QueryWorkbenchMemory(
            WorkbenchMemoryQuery::new(conversation.query(), 0, 0, false).expect("query")
        )
    );
    respond(&mut model, &inspect, memory_response(&inspect, 0, Vec::new()));

    let (save_request, save_command) =
        entered(&mut model, "/memory save Run focused acceptance tests.".to_owned());
    let WorkbenchIntent::SaveGuidance(save) = save_command.intent() else { panic!("save intent") };
    assert_eq!(save.expected_dependency_revision(), 0);
    assert_eq!(save.content().text().as_str(), "Run focused acceptance tests.");
    assert_eq!(save.content().source(), WorkbenchGuidanceSource::UserAuthored);
    assert_eq!(save.content().scope(), WorkbenchGuidanceScope::Project);
    assert!(!save.pinned());
    let saved = WorkbenchGuidanceRecord::save(
        save_command.operation(),
        save_command.query().workspace(),
        save.clone(),
    )
    .expect("saved record");
    accept_and_refresh(
        &mut model,
        &save_request,
        &save_command,
        1,
        vec![WorkbenchMemoryRow::Active(saved.clone())],
    );

    let id = crate::model::format_id(saved.identity().id().as_bytes());
    let (revise_request, revise_command) =
        entered(&mut model, format!("/memory revise {id} Run exact focused acceptance tests."));
    let WorkbenchIntent::ReviseGuidance(revision) = revise_command.intent() else {
        panic!("revise intent")
    };
    assert_eq!(revision.selection().expected_revision(), 1);
    assert_eq!(revision.expected_dependency_revision(), 1);
    assert_eq!(revision.content().source(), WorkbenchGuidanceSource::UserAuthored);
    let revised =
        saved.revise(revise_command.operation(), revision.clone()).expect("revised record");
    accept_and_refresh(
        &mut model,
        &revise_request,
        &revise_command,
        2,
        vec![WorkbenchMemoryRow::Active(revised.clone())],
    );

    let (pin_request, pin_command) = entered(&mut model, format!("/memory pin {id}"));
    let WorkbenchIntent::PinGuidance(pin) = pin_command.intent() else { panic!("pin intent") };
    assert_eq!(pin.expected_dependency_revision(), 2);
    assert!(pin.pinned());
    let pinned = revised.set_pinned(*pin).expect("pinned record");
    accept_and_refresh(
        &mut model,
        &pin_request,
        &pin_command,
        3,
        vec![WorkbenchMemoryRow::Active(pinned.clone())],
    );

    let (scope_request, scope_command) = entered(&mut model, format!("/memory conversation {id}"));
    let WorkbenchIntent::ScopeGuidance(scope) = scope_command.intent() else {
        panic!("scope intent")
    };
    assert_eq!(
        scope.scope(),
        WorkbenchGuidanceScope::Conversation(conversation.query().conversation())
    );
    let scoped = pinned.set_scope(*scope).expect("scoped record");
    accept_and_refresh(
        &mut model,
        &scope_request,
        &scope_command,
        4,
        vec![WorkbenchMemoryRow::Active(scoped.clone())],
    );

    let (forget_request, forget_command) =
        entered(&mut model, format!("/memory forget {id} Superseded by a reviewed project rule."));
    let WorkbenchIntent::ForgetGuidance(forget) = forget_command.intent() else {
        panic!("forget intent")
    };
    assert_eq!(forget.expected_dependency_revision(), 4);
    let tombstone = scoped.forget(forget_command.operation(), forget.clone()).expect("tombstone");
    assert_eq!(tombstone.lifecycle(), WorkbenchGuidanceLifecycle::Forgotten);
    accept_and_refresh(&mut model, &forget_request, &forget_command, 5, Vec::new());

    key(&mut model, KeyCode::Esc);
    let history = begin_memory_inspection(&mut model, "/memory history", conversation.query(), 6);
    let AppRequestPayload::QueryWorkbenchMemory(history_query) = history.payload() else {
        panic!("history query")
    };
    assert!(history_query.include_forgotten());
    respond(
        &mut model,
        &history,
        memory_response(&history, 5, vec![WorkbenchMemoryRow::Forgotten(tombstone)]),
    );

    assert_content_free_history(&model);
}

#[test]
fn queue_to_memory_refreshes_the_authoritative_aggregate_before_a_guidance_command() {
    let mut model = memory_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchInputs)
            .expect("queue feature"),
    );
    let conversation = open_conversation(&mut model);

    key(&mut model, KeyCode::Esc);
    let queue_request = request(&model.slash_command("/queue"));
    let AppRequestPayload::QueryWorkbenchQueue(queue_query) = queue_request.payload() else {
        panic!("queue query")
    };
    let current_queue = WorkbenchQueueQuery::new(queue_query.query(), 5, 0, false).expect("query");
    respond(
        &mut model,
        &queue_request,
        AppResponsePayload::WorkbenchQueue(
            WorkbenchQueuePage::new(current_queue, 0, Vec::new()).expect("queue page"),
        ),
    );
    assert_eq!(
        model.chat.workbench.snapshot.as_ref().expect("stale snapshot fixture").revision(),
        1
    );

    key(&mut model, KeyCode::Esc);
    let snapshot_request = request(&model.slash_command("/memory"));
    assert_eq!(
        snapshot_request.payload(),
        &AppRequestPayload::QueryWorkbench(conversation.query())
    );
    let memory_request = request(&respond(
        &mut model,
        &snapshot_request,
        AppResponsePayload::Workbench(snapshot(conversation.query(), 5)),
    ));
    respond(&mut model, &memory_request, memory_response(&memory_request, 0, Vec::new()));

    key(&mut model, KeyCode::Esc);
    let save_request = request(&model.slash_command("/memory save Keep the verified queue order."));
    let AppRequestPayload::WorkbenchCommand(save) = save_request.payload() else {
        panic!("guidance command")
    };
    assert_eq!(save.expected_revision(), 5);
    let WorkbenchIntent::SaveGuidance(guidance) = save.intent() else {
        panic!("save guidance intent")
    };
    assert_eq!(guidance.expected_dependency_revision(), 0);
}
