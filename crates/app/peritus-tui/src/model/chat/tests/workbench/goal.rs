use super::*;
mod criteria;
mod recovery;
mod refresh;
use peritus_app_protocol::{
    ControlOperationId, ProductInteractionMode, WorkbenchBrief, WorkbenchBriefEntry,
    WorkbenchBriefField, WorkbenchGoalBudget, WorkbenchGoalCriterion,
    WorkbenchGoalCriterionDefinition, WorkbenchGoalCriterionKind, WorkbenchGoalCriterionState,
    WorkbenchGoalPauseMode, WorkbenchGoalRole, WorkbenchGoalRoleUsage, WorkbenchGoalSnapshot,
    WorkbenchGoalState, WorkbenchGoalUsage, WorkbenchInputId, WorkbenchInputOrder,
    WorkbenchInputRow, WorkbenchInputSelection, WorkbenchInputState, WorkbenchInputText,
    WorkbenchIntent, WorkbenchQuery,
};

fn goal_model() -> AppModel {
    let mut model = enabled_model();
    model.features.extend([
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchBrief)
            .expect("brief feature"),
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchGoals)
            .expect("goal feature"),
    ]);
    model.chat.workbench.selected = Some(WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new([71; 16]).expect("conversation"),
        model.product.as_ref().expect("product").launch.workspace_id(),
    ));
    model
}

fn objective_brief(query: WorkbenchQuery, text: &str, revision: u64) -> WorkbenchBrief {
    let source = WorkbenchInputRow::new(
        WorkbenchInputSelection::new(WorkbenchInputId::new([72; 16]).unwrap(), 1).unwrap(),
        WorkbenchInputText::new(text.to_owned()).unwrap(),
        WorkbenchInputState::Queued,
        WorkbenchInputOrder::new(Vec::new()).unwrap(),
    )
    .unwrap();
    WorkbenchBrief::new(
        query,
        revision,
        vec![WorkbenchBriefEntry::new(WorkbenchBriefField::Objective, source).unwrap()],
    )
    .unwrap()
}

fn usage(tokens: Option<u64>, cost: Option<u64>) -> WorkbenchGoalUsage {
    WorkbenchGoalUsage::new(
        [
            WorkbenchGoalRoleUsage::new(WorkbenchGoalRole::Writer, 1, 1, 2, tokens, cost),
            WorkbenchGoalRoleUsage::new(WorkbenchGoalRole::Reviewer, 0, 0, 0, Some(0), Some(0)),
            WorkbenchGoalRoleUsage::new(WorkbenchGoalRole::Fixer, 0, 0, 0, Some(0), Some(0)),
        ],
        25,
        40,
        1,
        0,
        0,
        900,
        100,
        2_048,
    )
}

fn snapshot(
    query: WorkbenchQuery,
    revision: u64,
    state: WorkbenchGoalState,
) -> WorkbenchGoalSnapshot {
    snapshot_with_identity(query, revision, state, [73; 16])
}

fn snapshot_with_identity(
    query: WorkbenchQuery,
    revision: u64,
    state: WorkbenchGoalState,
    identity: [u8; 16],
) -> WorkbenchGoalSnapshot {
    let runner = WorkbenchGoalCriterionDefinition::new(
        WorkbenchGoalCriterionKind::RunnerAcceptance,
        WorkbenchInputText::new("Strict gate".to_owned()).unwrap(),
        true,
    );
    WorkbenchGoalSnapshot::new(
        query,
        revision,
        ControlOperationId::new(identity).unwrap(),
        RunId::new([74; 16]).unwrap(),
        WorkbenchInputText::new("Ship exact change".to_owned()).unwrap(),
        state,
        "Current exact state".to_owned(),
        2,
        1,
        state == WorkbenchGoalState::Active,
        None,
        vec![WorkbenchGoalCriterion::new(runner, WorkbenchGoalCriterionState::Pending, None)],
        WorkbenchGoalBudget::new(Some(60_000), Some(8), Some(20), Some(100_000)).unwrap(),
        usage(Some(50), Some(10)),
    )
    .unwrap()
}

#[test]
fn goal_clear_confirmation_survives_accounting_polls_only_for_the_same_goal() {
    let mut model = goal_model();
    let query = model.chat.workbench.selected.unwrap();
    let goal = snapshot(query, 17, WorkbenchGoalState::Active);
    model.chat.run_id = Some(goal.run());
    model.chat.workbench.goal = Some(goal);
    model.chat.buffer = "/goal clear".into();
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert!(model.chat.workbench.goal_clear_pending);
    model.accept_workbench_goal(query, snapshot(query, 18, WorkbenchGoalState::Active));
    assert!(
        model.chat.workbench.goal_clear_pending,
        "routine accounting must not revoke confirmation"
    );
    assert!(model.chat.buffer.is_empty(), "confirmation instruction consumes its slash command");
    assert!(model.chat.workbench.message.contains("/goal clear confirm"));
    model.accept_workbench_goal(
        query,
        snapshot_with_identity(query, 19, WorkbenchGoalState::Active, [75; 16]),
    );
    assert!(
        !model.chat.workbench.goal_clear_pending,
        "a different goal requires fresh confirmation"
    );
}

#[test]
fn live_goal_accounting_is_polled_with_its_execution() {
    let mut model = goal_model();
    let query = model.chat.workbench.selected.unwrap();
    let goal = snapshot(query, 17, WorkbenchGoalState::Active);
    model.chat.run_id = Some(goal.run());
    model.chat.workbench.goal = Some(goal);
    model.chat.workbench.goal_mode = true;
    let effects = model.poll_chat();
    assert_eq!(effects.len(), 2);
    assert!(effects.iter().any(|effect| matches!(effect,
        Effect::Send(AppMessage::Request(request)) if matches!(request.payload(), AppRequestPayload::QueryWorkbenchGoal(actual) if *actual == query)
    )));
}

#[test]
fn new_conversation_cannot_inherit_the_old_goal_or_reject_a_fresh_objective() {
    let mut model = goal_model();
    let previous = model.chat.workbench.selected.unwrap();
    model.chat.workbench.goal = Some(snapshot(previous, 17, WorkbenchGoalState::WaitingForUser));
    model.chat.run_id = Some(RunId::new([74; 16]).unwrap());
    let (sent, command) = create(&mut model);
    let refresh = request(&respond(&mut model, &sent, receipt(&command)));
    respond(
        &mut model,
        &refresh,
        AppResponsePayload::Workbench(
            WorkbenchSnapshot::new(
                command.query(),
                1,
                ConversationTitle::new("New task".to_owned()).unwrap(),
                false,
                false,
            )
            .unwrap(),
        ),
    );
    assert!(model.chat.workbench.goal.is_none());
    assert!(model.chat.run_id.is_none());
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/goal Read the converter".to_owned();
    let request = request(&key(&mut model, KeyCode::Enter));
    assert!(
        matches!(request.payload(), AppRequestPayload::QueryWorkbenchBrief(query) if *query == command.query())
    );
}

#[test]
fn goal_drafts_then_confirms_the_exact_brief_and_existing_runner_settings() {
    let mut model = goal_model();
    model.chat.mode = ProductInteractionMode::Build;
    model.chat.buffer = "/goal Ship exact change".to_owned();
    let inspect = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::QueryWorkbenchBrief(query) = inspect.payload() else {
        panic!("goal draft must inspect the brief")
    };
    assert!(model.chat.workbench.goal_draft.is_some());
    assert!(model.chat.run_id.is_none());
    respond(
        &mut model,
        &inspect,
        AppResponsePayload::WorkbenchBrief(objective_brief(*query, "Ship exact change", 9)),
    );

    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/goal confirm".to_owned();
    let start = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::WorkbenchCommand(command) = start.payload() else {
        panic!("typed goal command")
    };
    assert_eq!(command.expected_revision(), 9);
    let WorkbenchIntent::StartGoal { definition, settings } = command.intent() else {
        panic!("start goal intent")
    };
    assert_eq!(definition.objective().as_str(), "Ship exact change");
    assert_eq!(definition.criteria().len(), 1);
    assert!(definition.criteria()[0].mandatory());
    assert_eq!(definition.criteria()[0].kind(), WorkbenchGoalCriterionKind::RunnerAcceptance);
    assert_eq!(settings.mode(), ProductInteractionMode::Build);
    assert_eq!(settings.models(), &model.chat.models);
    assert_eq!(settings.providers(), model.chat_providers().unwrap());
    assert_eq!(model.chat.buffer, "/goal confirm", "receipt not yet durable");

    let refresh = request(&respond(&mut model, &start, receipt(command)));
    assert!(
        matches!(refresh.payload(), AppRequestPayload::QueryWorkbenchGoal(exact) if *exact == command.query())
    );
    assert!(model.chat.workbench.goal_draft.is_none());
    assert!(model.chat.buffer.is_empty());
    let goal = snapshot(command.query(), 10, WorkbenchGoalState::Active);
    let opened =
        request(&respond(&mut model, &refresh, AppResponsePayload::WorkbenchGoal(goal.clone())));
    assert!(
        matches!(opened.payload(), AppRequestPayload::QueryInteraction(query) if query.run_id() == goal.run())
    );
    assert_eq!(model.chat.run_id, Some(goal.run()));
}

#[test]
fn pause_resume_and_budget_use_the_goal_identity_and_aggregate_revision() {
    for (text, expected) in [
        ("/pause", "pause-after"),
        ("/pause now", "pause-now"),
        ("/pause before-edit", "pause-edit"),
        ("/resume", "resume"),
        ("/budget requests=12 tools=none", "budget"),
    ] {
        let mut model = goal_model();
        let query = model.chat.workbench.selected.unwrap();
        let current = snapshot(query, 17, WorkbenchGoalState::Active);
        let goal = current.goal();
        model.chat.workbench.goal = Some(current);
        model.chat.workbench.goal_mode = true;
        model.chat.buffer = text.to_owned();
        let sent = request(&key(&mut model, KeyCode::Enter));
        let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else {
            panic!("goal control")
        };
        assert_eq!(command.expected_revision(), 17);
        match (expected, command.intent()) {
            ("pause-after", WorkbenchIntent::PauseGoal { goal: exact, mode }) => {
                assert_eq!((*exact, *mode), (goal, WorkbenchGoalPauseMode::AfterOperation));
            }
            ("pause-now", WorkbenchIntent::PauseGoal { goal: exact, mode }) => {
                assert_eq!((*exact, *mode), (goal, WorkbenchGoalPauseMode::Now));
            }
            ("pause-edit", WorkbenchIntent::PauseGoal { goal: exact, mode }) => {
                assert_eq!((*exact, *mode), (goal, WorkbenchGoalPauseMode::BeforeEdit));
            }
            ("resume", WorkbenchIntent::ResumeGoal { goal: exact }) => assert_eq!(*exact, goal),
            ("budget", WorkbenchIntent::UpdateGoalBudget { goal: exact, budget }) => {
                assert_eq!(*exact, goal);
                assert_eq!(budget.max_requests(), Some(12));
                assert_eq!(budget.max_tool_calls(), None);
                assert_eq!(budget.max_active_millis(), Some(60_000));
                assert_eq!(budget.max_total_tokens(), Some(100_000));
            }
            _ => panic!("wrong goal intent for {text}: {:?}", command.intent()),
        }
    }
}

#[test]
fn goal_control_receipts_accept_intervening_usage_without_losing_the_user_draft() {
    for command_text in ["/pause now", "/budget time=2m"] {
        let mut model = goal_model();
        let query = model.chat.workbench.selected.unwrap();
        model.chat.workbench.goal = Some(snapshot(query, 17, WorkbenchGoalState::Active));
        model.chat.workbench.goal_mode = true;
        model.chat.buffer = command_text.to_owned();
        let sent = request(&key(&mut model, KeyCode::Enter));
        let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else {
            panic!("pause command")
        };
        model.chat.buffer = "Actually keep the existing layout".to_owned();
        let accepted = WorkbenchReceipt::new(
            command.operation(),
            query,
            24,
            peritus_types::Sha256Digest::new([9; 32]),
        )
        .unwrap();
        let refresh = respond(&mut model, &sent, AppResponsePayload::WorkbenchReceipt(accepted));
        assert!(!refresh.is_empty());
        assert!(model.chat.workbench.unresolved.is_none());
        assert_eq!(model.chat.buffer, "Actually keep the existing layout");
    }
}

#[test]
fn goal_panel_renders_unknown_usage_and_unavailable_later_phase_evidence() {
    use ratatui::{Terminal, backend::TestBackend};

    let mut model = goal_model();
    let query = model.chat.workbench.selected.unwrap();
    let mut goal = snapshot(query, 17, WorkbenchGoalState::Active);
    let graphical = WorkbenchGoalCriterionDefinition::new(
        WorkbenchGoalCriterionKind::GraphicalPlaytest,
        WorkbenchInputText::new("Exercise native controls".to_owned()).unwrap(),
        false,
    );
    goal = WorkbenchGoalSnapshot::new(
        query,
        goal.aggregate_revision(),
        goal.goal(),
        goal.run(),
        goal.objective().clone(),
        goal.state(),
        goal.reason().to_owned(),
        goal.user_revision(),
        goal.attempt(),
        goal.restart_eligible(),
        goal.pause_mode(),
        vec![
            goal.criteria()[0].clone(),
            WorkbenchGoalCriterion::new(graphical, WorkbenchGoalCriterionState::Unavailable, None),
        ],
        goal.budget(),
        usage(None, None),
    )
    .unwrap();
    model.chat.workbench.goal = Some(goal);
    model.chat.workbench.goal_mode = true;
    model.chat.workbench.open = true;
    let mut terminal = Terminal::new(TestBackend::new(120, 38)).unwrap();
    let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).unwrap();
    let text: String = frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect();
    for expected in [
        "Goal · durable execution control",
        "unavailable",
        "tokens unknown",
        "cost unknown",
        "Unknown token or cost",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
}

#[test]
fn goal_confirmation_saves_a_missing_brief_objective_then_starts_only_after_its_receipt() {
    let mut model = goal_model();
    model.chat.buffer = "/goal Ship exact change".to_owned();
    let inspect = request(&key(&mut model, KeyCode::Enter));
    let query = model.chat.workbench.selected.unwrap();
    respond(
        &mut model,
        &inspect,
        AppResponsePayload::WorkbenchBrief(WorkbenchBrief::new(query, 1, vec![]).unwrap()),
    );
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/goal confirm".to_owned();
    let save = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::WorkbenchCommand(command) = save.payload() else {
        panic!("brief command")
    };
    assert!(
        matches!(command.intent(), WorkbenchIntent::SetBrief { field: WorkbenchBriefField::Objective, text } if text.as_str() == "Ship exact change")
    );
    assert!(model.chat.run_id.is_none());
    assert!(model.chat.workbench.goal_confirm_pending.is_some());
    let refreshed = request(&respond(&mut model, &save, receipt(command)));
    assert!(matches!(refreshed.payload(), AppRequestPayload::QueryWorkbenchBrief(_)));
    let start = request(&respond(
        &mut model,
        &refreshed,
        AppResponsePayload::WorkbenchBrief(objective_brief(query, "Ship exact change", 2)),
    ));
    let AppRequestPayload::WorkbenchCommand(command) = start.payload() else {
        panic!("goal command")
    };
    assert_eq!(command.expected_revision(), 2);
    assert!(
        matches!(command.intent(), WorkbenchIntent::StartGoal { definition, .. } if definition.objective().as_str() == "Ship exact change")
    );
    assert!(model.chat.workbench.goal_confirm_pending.is_none());
    assert!(model.chat.run_id.is_none(), "goal receipt is still pending");
}
