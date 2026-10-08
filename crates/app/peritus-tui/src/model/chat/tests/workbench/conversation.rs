use super::*;
use peritus_app_protocol::{
    ProductInteractionQuery, ProductRunControlAction, ProductRunLegalControls, ProductRunOperation,
    ProductRunOperationKind, ProductRunOperationState, WorkbenchExecutionState, WorkbenchIntent,
    WorkbenchQuery, WorkbenchQueueIntent,
};

mod steering;

fn chat_model() -> AppModel {
    let mut model = enabled_model();
    enable_durable_chat(&mut model);
    model
}

#[test]
fn composer_queues_in_selected_session_then_starts_only_after_receipt() {
    let mut model = chat_model();
    let (create_request, create_command) = create(&mut model);
    let refresh = request(&respond(&mut model, &create_request, receipt(&create_command)));
    let query = create_command.query();
    respond(&mut model, &refresh, AppResponsePayload::Workbench(metadata(query, 1)));
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "Read my selected attachment, and explain it.".to_owned();
    let lookup = request(&key(&mut model, KeyCode::Enter));
    assert_eq!(lookup.payload(), &AppRequestPayload::QueryWorkbenchExecution(query));
    assert!(!model.chat.buffer.is_empty());
    let enqueue = request(&respond(&mut model, &lookup, state(query, 1, None)));
    let AppRequestPayload::WorkbenchCommand(command) = enqueue.payload() else { panic!("queue") };
    assert_eq!(command.query(), query);
    assert!(matches!(command.intent(), WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(_))));
    let lookup = request(&respond(&mut model, &enqueue, receipt(command)));
    assert!(model.chat.buffer.is_empty(), "receipt clears exactly the submitted draft");
    let start = request(&respond(&mut model, &lookup, state(query, 2, None)));
    let AppRequestPayload::WorkbenchCommand(command) = start.payload() else { panic!("start") };
    let WorkbenchIntent::StartExecution(settings) = command.intent() else {
        panic!("start intent")
    };
    assert_eq!(command.query(), query);
    assert_eq!(command.expected_revision(), 2);
    let interaction = request(&respond(&mut model, &start, receipt(command)));
    assert_eq!(
        interaction.payload(),
        &AppRequestPayload::QueryInteraction(ProductInteractionQuery::new(settings.run()))
    );
    assert_eq!(model.chat.run_id, Some(settings.run()));
}

#[test]
fn reopening_a_durable_session_discovers_its_run_without_execution() {
    let mut model = chat_model();
    let query = WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new([8; 16]).expect("id"),
        WorkspaceId::new([4; 16]).expect("workspace"),
    );
    model.select_workbench_conversation(Some(query));
    let lookup = request(&model.discover_workbench_execution());
    let run = RunId::new([9; 16]).expect("run");
    let observed = request(&respond(&mut model, &lookup, state(query, 8, Some(run))));
    assert_eq!(
        observed.payload(),
        &AppRequestPayload::QueryInteraction(ProductInteractionQuery::new(run))
    );
    assert_eq!(model.chat.run_id, Some(run));
}

fn metadata(query: WorkbenchQuery, revision: u64) -> WorkbenchSnapshot {
    WorkbenchSnapshot::new(
        query,
        revision,
        ConversationTitle::new("Fixture".to_owned()).expect("title"),
        false,
        false,
    )
    .expect("snapshot")
}
fn state(query: WorkbenchQuery, revision: u64, run: Option<RunId>) -> AppResponsePayload {
    AppResponsePayload::WorkbenchExecution(
        WorkbenchExecutionState::new(metadata(query, revision), run, false).expect("state"),
    )
}

#[test]
fn pending_chat_input_does_not_resume_an_unknown_command_outcome() {
    let mut model = chat_model();
    let workspace = model.product.as_ref().expect("product").launch.workspace_id();
    let query = WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new([8; 16]).expect("conversation"),
        workspace,
    );
    let run = RunId::new([9; 16]).expect("run");
    model.select_workbench_conversation(Some(query));
    model.chat.buffer = "Continue after checking the interrupted command.".to_owned();
    let lookup = request(&key(&mut model, KeyCode::Enter));
    let enqueue = request(&respond(&mut model, &lookup, state(query, 1, Some(run))));
    let AppRequestPayload::WorkbenchCommand(queued) = enqueue.payload() else { panic!("queue") };
    let lookup = request(&respond(&mut model, &enqueue, receipt(queued)));
    let interaction = request(&respond(&mut model, &lookup, state(query, 2, Some(run))));
    let snapshot = peritus_app_protocol::ProductRunSnapshot::new(
        run,
        workspace,
        model.chat_providers().expect("providers"),
        peritus_app_protocol::ProductRunPhase::RecoveryRequired,
        1,
        "Interrupted command".to_owned(),
        "Outcome requires inspection".to_owned(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        ProductRunOperation::new(
            ProductRunOperationKind::Command,
            ProductRunOperationState::OutcomeUnknown,
            "command/exact".to_owned(),
            "The original command receipt is durable.".to_owned(),
            "The host cannot prove whether the command changed the workspace.".to_owned(),
            ProductRunLegalControls::none().with(ProductRunControlAction::Acknowledge),
        )
        .expect("operation"),
    )
    .expect("snapshot");
    let payload = AppResponsePayload::Interaction(
        ProductInteractionSnapshot::new(
            snapshot,
            ProductInteractionMode::Chat,
            ProductRoleModels::default(),
            2,
            1,
            Vec::new(),
            None,
        )
        .expect("interaction"),
    );
    assert!(respond(&mut model, &interaction, payload).is_empty());
    assert!(!model.workbench_chat_starting());
    assert!(model.notice.as_ref().expect("notice").text.contains("must be reconciled"));
}

#[test]
fn new_chat_detaches_durable_session_but_cannot_abandon_an_input_receipt() {
    let mut model = chat_model();
    let query = WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new([8; 16]).expect("id"),
        WorkspaceId::new([4; 16]).expect("workspace"),
    );
    model.select_workbench_conversation(Some(query));
    model.chat.buffer = "An input to this session".to_owned();
    let lookup = request(&key(&mut model, KeyCode::Enter));
    model.chat.buffer = "/new".to_owned();
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert_eq!(model.chat.workbench.selected, Some(query));
    let enqueue = request(&respond(&mut model, &lookup, state(query, 1, None)));
    assert!(matches!(enqueue.payload(), AppRequestPayload::WorkbenchCommand(_)));
    assert!(model.chat_submission_pending());
    respond(
        &mut model,
        &enqueue,
        AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
            peritus_app_protocol::AppErrorCode::StaleRevision,
            None,
        )),
    );
    assert!(!model.chat_submission_pending());
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/new".to_owned();
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert!(model.chat.workbench.selected.is_none());
    assert!(model.chat.run_id.is_none());
}

#[test]
fn stop_at_each_admission_boundary_never_loses_receipts_or_restarts_work() {
    for boundary in 0..3 {
        let mut model = chat_model();
        let workspace = model.product.as_ref().expect("product").launch.workspace_id();
        let query = WorkbenchQuery::new(
            peritus_app_protocol::ConversationId::new([8; 16]).expect("id"),
            workspace,
        );
        model.select_workbench_conversation(Some(query));
        model.chat.buffer = "Please inspect the source.".to_owned();
        let lookup = request(&key(&mut model, KeyCode::Enter));
        assert!(model.chat_work_active());
        if boundary == 0 {
            assert!(model.chat_control(ProductRunControlAction::Cancel).is_empty());
            assert!(respond(&mut model, &lookup, state(query, 1, None)).is_empty());
            assert_eq!(model.chat.buffer, "Please inspect the source.");
            assert!(!model.chat_submission_pending());
            continue;
        }
        let enqueue = request(&respond(&mut model, &lookup, state(query, 1, None)));
        let AppRequestPayload::WorkbenchCommand(queued) = enqueue.payload() else {
            panic!("queue")
        };
        if boundary == 1 {
            assert!(model.chat_control(ProductRunControlAction::Cancel).is_empty());
        }
        let lookup = request(&respond(&mut model, &enqueue, receipt(queued)));
        if boundary == 1 {
            assert!(respond(&mut model, &lookup, state(query, 2, None)).is_empty());
            assert!(!model.chat_submission_pending());
            assert!(model.chat.workbench.unresolved.is_none());
            continue;
        }
        let start = request(&respond(&mut model, &lookup, state(query, 2, None)));
        let AppRequestPayload::WorkbenchCommand(command) = start.payload() else { panic!("start") };
        let WorkbenchIntent::StartExecution(settings) = command.intent() else {
            panic!("settings")
        };
        assert!(model.chat_control(ProductRunControlAction::Cancel).is_empty());
        let cancel = request(&respond(&mut model, &start, receipt(command)));
        assert!(matches!(cancel.payload(), AppRequestPayload::ControlProductRun(control)
            if control.run_id() == settings.run() && control.action() == ProductRunControlAction::Cancel));
        assert!(model.chat.workbench.unresolved.is_none());
        assert!(!model.workbench_chat_starting());
    }
}

#[test]
fn stop_while_continuation_acknowledgement_is_pending_cancels_the_bound_run() {
    let mut model = chat_model();
    let workspace = model.product.as_ref().expect("product").launch.workspace_id();
    let query = WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new([8; 16]).expect("id"),
        workspace,
    );
    let run = RunId::new([9; 16]).expect("run");
    model.select_workbench_conversation(Some(query));
    model.chat.buffer = "Change my previous plan.".to_owned();
    let lookup = request(&key(&mut model, KeyCode::Enter));
    let enqueue = request(&respond(&mut model, &lookup, state(query, 1, Some(run))));
    let AppRequestPayload::WorkbenchCommand(queued) = enqueue.payload() else { panic!("queue") };
    let lookup = request(&respond(&mut model, &enqueue, receipt(queued)));
    let interaction = request(&respond(&mut model, &lookup, state(query, 2, Some(run))));
    let snapshot = peritus_app_protocol::ProductRunSnapshot::new(
        run,
        workspace,
        model.chat_providers().expect("providers"),
        peritus_app_protocol::ProductRunPhase::Complete,
        1,
        "Previous request".to_owned(),
        "Complete".to_owned(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        crate::test_support::run_operation(run, peritus_app_protocol::ProductRunPhase::Complete),
    )
    .expect("snapshot");
    let payload = AppResponsePayload::Interaction(
        ProductInteractionSnapshot::new(
            snapshot,
            ProductInteractionMode::Chat,
            ProductRoleModels::default(),
            1,
            1,
            Vec::new(),
            None,
        )
        .expect("interaction"),
    );
    let continuation = request(&respond(&mut model, &interaction, payload));
    let AppRequestPayload::WorkbenchCommand(continued) = continuation.payload() else {
        panic!("durable continuation")
    };
    assert!(matches!(continued.intent(), WorkbenchIntent::ContinueExecution(_)));
    assert!(model.chat_control(ProductRunControlAction::Cancel).is_empty());
    let admission_query = request(&respond(&mut model, &continuation, receipt(continued)));
    assert_eq!(
        admission_query.payload(),
        &AppRequestPayload::QueryWorkbenchContinuationAdmission(continued.clone()),
    );
    let admission = peritus_app_protocol::WorkbenchContinuationAdmission::new(
        continued.operation(),
        continued.query(),
        run,
        peritus_app_protocol::WorkbenchContinuationAdmissionState::LaunchOwned,
    );
    let cancel = request(&respond(
        &mut model,
        &admission_query,
        AppResponsePayload::WorkbenchContinuationAdmission(admission),
    ));
    assert!(matches!(cancel.payload(), AppRequestPayload::ControlProductRun(control)
        if control.run_id() == run && control.action() == ProductRunControlAction::Cancel));
    assert!(!model.workbench_chat_starting());
}

#[test]
fn ordinary_new_chat_creates_a_session_before_queuing_and_can_stop_at_creation() {
    for stop in [false, true] {
        let mut model = chat_model();
        model.chat.buffer = "Read the README and explain it".to_owned();
        let creation = request(&key(&mut model, KeyCode::Enter));
        let AppRequestPayload::WorkbenchCommand(command) = creation.payload() else {
            panic!("durable creation")
        };
        assert!(matches!(command.intent(), WorkbenchIntent::CreateConversation(_)));
        assert_eq!(command.expected_revision(), 0);
        assert_eq!(model.chat.workbench.selected, Some(command.query()));
        if stop {
            assert!(model.chat_control(ProductRunControlAction::Cancel).is_empty());
        }
        let lookup = request(&respond(&mut model, &creation, receipt(command)));
        assert_eq!(
            model.chat.buffer, "Read the README and explain it",
            "creation is not input acceptance"
        );
        let effects = respond(&mut model, &lookup, state(command.query(), 1, None));
        if stop {
            assert!(effects.is_empty());
            assert!(!model.workbench_chat_pending());
            assert!(!model.chat.buffer.is_empty());
        } else {
            let enqueue = request(&effects);
            let AppRequestPayload::WorkbenchCommand(queued) = enqueue.payload() else {
                panic!("queue")
            };
            assert!(matches!(
                queued.intent(),
                WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(_))
            ));
            respond(&mut model, &enqueue, receipt(queued));
            assert!(model.chat.buffer.is_empty());
        }
    }
}
