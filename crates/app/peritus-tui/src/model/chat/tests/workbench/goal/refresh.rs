use super::*;

#[test]
fn budget_after_a_brief_edit_refreshes_its_goal_without_losing_or_replacing_drafts() {
    for changed_mind in [0, 1, 2, 3] {
        let mut model = goal_model();
        let query = model.chat.workbench.selected.unwrap();
        model.chat.workbench.goal = Some(snapshot(query, 17, WorkbenchGoalState::Paused));
        model.chat.buffer = "/brief".to_owned();
        let inspect = request(&key(&mut model, KeyCode::Enter));
        respond(
            &mut model,
            &inspect,
            AppResponsePayload::WorkbenchBrief(objective_brief(query, "Ship exact change", 17)),
        );
        key(&mut model, KeyCode::Esc);
        model.chat.buffer = "/brief constraints Preserve my draft".to_owned();
        let edit = request(&key(&mut model, KeyCode::Enter));
        let AppRequestPayload::WorkbenchCommand(command) = edit.payload() else {
            panic!("brief edit")
        };
        let refresh = request(&respond(&mut model, &edit, receipt(command)));
        respond(
            &mut model,
            &refresh,
            AppResponsePayload::WorkbenchBrief(objective_brief(query, "Ship exact change", 18)),
        );
        key(&mut model, KeyCode::Esc);
        model.chat.buffer = "/budget time=5m".to_owned();
        let inspect = request(&key(&mut model, KeyCode::Enter));
        assert!(
            matches!(inspect.payload(), AppRequestPayload::QueryWorkbenchGoal(actual) if *actual == query)
        );
        if changed_mind == 1 {
            model.chat.buffer = "Actually leave the budget alone".to_owned();
        } else if changed_mind == 2 {
            key(&mut model, KeyCode::Esc);
        } else if changed_mind == 3 {
            model.pending_started.retain(|id, _| *id == inspect.request_id());
            model.tick_count = 120;
            assert!(!model.expire_pending_requests());
        }
        let effects = respond(
            &mut model,
            &inspect,
            AppResponsePayload::WorkbenchGoal(snapshot(query, 18, WorkbenchGoalState::Paused)),
        );
        if changed_mind != 0 {
            assert!(effects.is_empty());
            assert_eq!(
                model.chat.buffer,
                if changed_mind == 1 {
                    "Actually leave the budget alone"
                } else {
                    "/budget time=5m"
                }
            );
            assert!(model.chat.workbench.unresolved.is_none());
        } else {
            let edit = request(&effects);
            let AppRequestPayload::WorkbenchCommand(command) = edit.payload() else {
                panic!("budget edit")
            };
            assert_eq!(command.expected_revision(), 18);
            assert!(
                matches!(command.intent(), WorkbenchIntent::UpdateGoalBudget { budget, .. } if budget.max_active_millis() == Some(300_000))
            );
            assert_eq!(model.chat.buffer, "/budget time=5m");
            respond(&mut model, &edit, receipt(command));
            assert!(model.chat.buffer.is_empty());
        }
    }
}
