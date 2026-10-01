use super::*;

fn press(model: &mut AppModel, code: KeyCode) -> Vec<Effect> {
    model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(code, KeyModifiers::NONE))))
}

#[test]
fn preview_refresh_opens_selected_results_without_a_prior_slash_command() {
    let (mut model, query, run, _) = preview_model();
    model.view = View::Runs;
    press(&mut model, KeyCode::Char('8'));
    let sent = request(&press(&mut model, KeyCode::Char('r')));
    assert_eq!(
        sent.payload(),
        &AppRequestPayload::QueryWorkbenchResult(WorkbenchResultQuery::new(query, run))
    );
}

#[test]
fn offline_preview_refresh_does_not_claim_a_request_was_sent() {
    let (mut model, query, run, _) = preview_model();
    model.view = View::Preview;
    model.product.as_mut().unwrap().preview_query = Some(WorkbenchResultQuery::new(query, run));
    model.update(Action::Disconnected("fixture disconnect".to_owned()));
    assert!(press(&mut model, KeyCode::Char('r')).is_empty());
    assert_ne!(model.product.as_ref().unwrap().preview_message, "Refreshing preview…");
    assert!(model.notice.as_ref().is_some_and(|notice| notice.text.contains("offline")));
}

#[test]
fn preview_scroll_stops_at_content_and_reverses_immediately() {
    let (mut model, query, run, _) = preview_model();
    model.chat.buffer = "/preview launch python3 demo.py".to_owned();
    let sent = request(&model.slash_command(&model.chat.buffer.clone()));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else { panic!("command") };
    let WorkbenchIntent::StartPreview(profile) = command.intent() else { panic!("profile") };
    let refresh = request(&respond(&mut model, &sent, preview_receipt(command)));
    respond(
        &mut model,
        &refresh,
        AppResponsePayload::WorkbenchResult(result_page(
            WorkbenchResultQuery::new(query, run),
            command.operation(),
            profile.clone(),
        )),
    );
    model.product.as_mut().unwrap().preview_outputs.push(
        peritus_app_protocol::WorkbenchPreviewOutput::new(
            command.operation(),
            format!("{}OUTPUT_TAIL", "a line of preview output\n".repeat(80)),
            String::new(),
            false,
        )
        .expect("output"),
    );
    for (width, height) in [(48, 12), (80, 24), (120, 38)] {
        model.chat.viewport = Some(ratatui::layout::Rect::new(0, 0, width, height));
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))
                .expect("terminal");
        for _ in 0..100 {
            press(&mut model, KeyCode::PageDown);
        }
        let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).expect("draw");
        let bottom = frame.buffer.clone();
        let text = bottom.content().iter().map(ratatui::buffer::Cell::symbol).collect::<String>();
        assert!(text.contains("OUTPUT_TAIL"), "end of preview missing at {width}x{height}: {text}");
        press(&mut model, KeyCode::PageUp);
        let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).expect("draw");
        assert_ne!(frame.buffer, &bottom, "up must move immediately at {width}x{height}");
    }
}
