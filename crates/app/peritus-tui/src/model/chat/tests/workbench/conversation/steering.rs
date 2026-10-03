//! Steering stays in the transcript while admission and recovery retain their exact receipts.

use super::*;
use peritus_app_protocol::{ProductActivity, ProductActivityKind, ProductInteractionSnapshot};
use ratatui::{Terminal, backend::TestBackend};

fn running_chat() -> (AppModel, WorkbenchQuery, RunId) {
    let mut model = chat_model();
    let workspace = model.product.as_ref().unwrap().launch.workspace_id();
    let query = WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new([81; 16]).unwrap(),
        workspace,
    );
    let run = RunId::new([82; 16]).unwrap();
    model.select_workbench_conversation(Some(query));
    model.chat.run_id = Some(run);
    model.chat.snapshot = Some(interaction(&model, run));
    (model, query, run)
}

fn interaction(model: &AppModel, run: RunId) -> ProductInteractionSnapshot {
    let phase = peritus_app_protocol::ProductRunPhase::Writing;
    ProductInteractionSnapshot::new(
        peritus_app_protocol::ProductRunSnapshot::new(
            run,
            model.product.as_ref().unwrap().launch.workspace_id(),
            model.chat_providers().unwrap(),
            phase,
            1,
            "Existing conversation".to_owned(),
            "Working".to_owned(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            crate::test_support::run_operation(run, phase),
        )
        .unwrap(),
        ProductInteractionMode::Chat,
        ProductRoleModels::default(),
        1,
        1,
        vec![
            ProductActivity::new(
                1,
                ProductActivityKind::Assistant,
                "Existing reply stays visible while steering is saved.".to_owned(),
                String::new(),
            )
            .unwrap(),
        ],
        None,
    )
    .unwrap()
}

fn assert_transcript(model: &AppModel) {
    assert!(!model.chat.workbench.open, "steering must not open an inspector");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let frame = terminal.draw(|frame| crate::render::draw(frame, model)).unwrap();
    let text: String = frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect();
    assert!(text.contains("Existing reply stays visible"));
    assert!(!text.contains("Sessions · metadata"));
    assert!(!text.contains("Awaiting durable receipt"));
}

#[test]
fn steering_keeps_the_transcript_visible_before_and_after_each_receipt() {
    for has_goal in [false, true] {
        let (mut model, query, run) = running_chat();
        model.chat.buffer = "Keep the existing behavior and explain the result.".to_owned();
        let lookup = request(&key(&mut model, KeyCode::Enter));
        assert_transcript(&model);
        let execution = AppResponsePayload::WorkbenchExecution(
            WorkbenchExecutionState::new(metadata(query, 1), Some(run), has_goal).unwrap(),
        );
        let enqueue = request(&respond(&mut model, &lookup, execution));
        assert_transcript(&model);
        for _ in 0..3 {
            assert_transcript(&model);
        }
        let AppRequestPayload::WorkbenchCommand(command) = enqueue.payload() else {
            panic!("enqueue")
        };
        assert!(matches!(
            command.intent(),
            WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(_))
        ));
        assert!(!model.chat.buffer.is_empty(), "pending input retains its draft");
        let lookup = request(&respond(&mut model, &enqueue, receipt(command)));
        assert_transcript(&model);
        assert!(model.chat.buffer.is_empty());
        let execution = AppResponsePayload::WorkbenchExecution(
            WorkbenchExecutionState::new(metadata(query, 2), Some(run), has_goal).unwrap(),
        );
        let observation = request(&respond(&mut model, &lookup, execution));
        assert_transcript(&model);
        let payload = AppResponsePayload::Interaction(interaction(&model, run));
        assert!(respond(&mut model, &observation, payload).is_empty());
        assert_transcript(&model);
        assert!(!model.workbench_chat_starting());
    }
}

#[test]
fn rejected_steering_keeps_the_transcript_and_original_draft() {
    let (mut model, query, run) = running_chat();
    let draft = "Retain this steering if admission fails.";
    model.chat.buffer = draft.to_owned();
    let lookup = request(&key(&mut model, KeyCode::Enter));
    let enqueue = request(&respond(&mut model, &lookup, state(query, 1, Some(run))));
    assert_transcript(&model);
    respond(
        &mut model,
        &enqueue,
        AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
            peritus_app_protocol::AppErrorCode::StaleRevision,
            None,
        )),
    );
    assert_transcript(&model);
    assert_eq!(model.chat.buffer, draft);
    let notice = model.notice.as_ref().expect("visible admission failure");
    assert_eq!(notice.level, NoticeLevel::Error);
    assert!(notice.text.contains("Refresh"));
    assert!(model.chat.workbench.message.contains("draft retained"));
    assert!(!model.workbench_chat_starting());
}
