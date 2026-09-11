//! Graphical criteria remain explicit local drafts until exact goal confirmation.

use super::*;

fn drafted(preview: bool) -> AppModel {
    let mut model = goal_model();
    if preview {
        model.features.push(
            ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchPreview).unwrap(),
        );
    }
    model.chat.mode = ProductInteractionMode::Build;
    model.chat.buffer = "/goal Ship exact change".to_owned();
    let inspect = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::QueryWorkbenchBrief(query) = inspect.payload() else {
        panic!("brief inspection");
    };
    respond(
        &mut model,
        &inspect,
        AppResponsePayload::WorkbenchBrief(objective_brief(*query, "Ship exact change", 9)),
    );
    key(&mut model, KeyCode::Esc);
    model
}

#[test]
fn explicit_graphical_criterion_is_local_and_confirmation_retains_both_mandatory_gates() {
    let mut model = drafted(true);
    model.chat.buffer = "/goal criterion graphical Exercise the native controls".to_owned();
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert!(model.chat.run_id.is_none());
    assert!(model.chat.workbench.unresolved.is_none());
    assert_eq!(
        model.chat.workbench.goal_draft.as_ref().unwrap().graphical().unwrap().as_str(),
        "Exercise the native controls"
    );
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 38)).unwrap();
    let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).unwrap();
    let text: String = frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect();
    assert!(text.contains("mandatory · graphical playtest"));
    assert!(text.contains("writer configured model"));
    assert!(text.contains("reviewer configured model"));
    assert!(text.contains("fixer configured model"));
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/goal confirm".to_owned();
    let sent = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else { panic!("command") };
    let WorkbenchIntent::StartGoal { definition, .. } = command.intent() else { panic!("goal") };
    assert_eq!(command.expected_revision(), 9);
    assert_eq!(definition.criteria().len(), 2);
    assert!(definition.criteria().iter().all(WorkbenchGoalCriterionDefinition::mandatory));
    assert_eq!(definition.criteria()[0].kind(), WorkbenchGoalCriterionKind::RunnerAcceptance);
    assert_eq!(definition.criteria()[1].kind(), WorkbenchGoalCriterionKind::GraphicalPlaytest);
    assert_eq!(definition.criteria()[1].description().as_str(), "Exercise the native controls");
}

#[test]
fn missing_feature_and_invalid_criterion_leave_the_goal_draft_unchanged() {
    for (preview, text) in [
        (false, "/goal criterion graphical Exercise controls"),
        (true, "/goal criterion graphical"),
        (true, "/goal criterion unknown Details"),
    ] {
        let mut model = drafted(preview);
        model.chat.buffer = text.to_owned();
        assert!(key(&mut model, KeyCode::Enter).is_empty());
        assert_eq!(model.chat.buffer, text);
        let draft = model.chat.workbench.goal_draft.as_ref().unwrap();
        assert_eq!(draft.objective().as_str(), "Ship exact change");
        assert!(draft.graphical().is_none());
        assert!(model.chat.workbench.unresolved.is_none());
    }
}

#[test]
fn removal_only_changes_the_unconfirmed_graphical_criterion() {
    let mut model = drafted(true);
    model.chat.buffer = "/goal criterion graphical Play the game".to_owned();
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/goal criterion remove-graphical".to_owned();
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    let draft = model.chat.workbench.goal_draft.as_ref().unwrap();
    assert!(draft.graphical().is_none());
    assert_eq!(draft.objective().as_str(), "Ship exact change");
    let query = model.chat.workbench.selected.unwrap();
    model.chat.workbench.goal = Some(snapshot(query, 9, WorkbenchGoalState::Active));
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/goal criterion graphical New requirement".to_owned();
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert!(model.chat.workbench.goal_draft.as_ref().unwrap().graphical().is_none());
    assert_eq!(model.chat.workbench.goal.as_ref().unwrap().aggregate_revision(), 9);
}

#[test]
fn preview_feature_loss_after_drafting_cannot_start_a_goal() {
    let mut model = drafted(true);
    model.chat.buffer = "/goal criterion graphical Play the game".to_owned();
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    model
        .features
        .retain(|feature| feature.as_str() != WellKnownProtocolFeature::WorkbenchPreview.as_str());
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/goal confirm".to_owned();
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert_eq!(model.chat.buffer, "/goal confirm");
    assert!(model.chat.workbench.unresolved.is_none());
    assert!(model.chat.workbench.goal_draft.as_ref().unwrap().graphical().is_some());
}
