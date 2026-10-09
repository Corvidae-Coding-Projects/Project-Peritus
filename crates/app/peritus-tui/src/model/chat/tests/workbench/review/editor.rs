use super::*;

#[test]
fn selected_hunk_comment_retains_rejected_draft_and_stale_comment_rebinds_explicitly() {
    let (mut model, query, run) = review_model();
    model.chat.buffer = "/diff".to_owned();
    let inspect = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::QueryWorkbenchReview(_requested) = inspect.payload() else {
        panic!("structured review query")
    };
    let first = page(query, run, 7, 10, "old", "new", &[]);
    respond(&mut model, &inspect, AppResponsePayload::WorkbenchReview(first));
    assert_eq!(model.view, View::Diff);

    key(&mut model, KeyCode::Tab); // file -> hunk
    key(&mut model, KeyCode::Char('e'));
    let editor = model.editor.as_mut().expect("review editor");
    editor.buffer = "Why is this necessary?".to_owned();
    editor.cursor = editor.buffer.len();
    let submit = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::WorkbenchCommand(add) = submit.payload() else { panic!("add") };
    let (comment_id, stale_anchor) = match add.intent() {
        WorkbenchIntent::AddReview { anchor, feedback, message } => {
            assert_eq!(*feedback, WorkbenchReviewFeedback::Explain);
            assert_eq!(message.as_str(), "Why is this necessary?");
            assert_eq!(anchor.target(), peritus_app_protocol::WorkbenchReviewTarget::Hunk);
            (add.operation(), anchor.clone())
        }
        _ => panic!("anchored add"),
    };
    respond(
        &mut model,
        &submit,
        AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
            AppErrorCode::StaleRevision,
            None,
        )),
    );
    assert_eq!(model.editor.as_ref().expect("retained editor").buffer, "Why is this necessary?");
    model.editor = None;

    let refresh = request(&model.refresh_review());
    let stale = WorkbenchReviewComment::new(
        comment_id,
        1,
        stale_anchor,
        WorkbenchReviewFeedback::Explain,
        WorkbenchInputText::new("Why is this necessary?".to_owned()).expect("message"),
        WorkbenchInputSelection::new(
            WorkbenchInputId::new(*comment_id.as_bytes()).expect("input"),
            1,
        )
        .expect("selection"),
        WorkbenchReviewCommentState::Stale,
    )
    .expect("comment");
    let current = page(query, run, 8, 11, "new", "newer", &[stale]);
    respond(&mut model, &refresh, AppResponsePayload::WorkbenchReview(current));
    key(&mut model, KeyCode::Tab); // hunk -> comment
    let rebind_request = request(&key(&mut model, KeyCode::Char('b')));
    let AppRequestPayload::WorkbenchCommand(rebind) = rebind_request.payload() else {
        panic!("rebind")
    };
    assert!(matches!(
        rebind.intent(),
        WorkbenchIntent::RebindReview { comment, anchor }
            if *comment == comment_id
                && anchor.candidate_digest() == Sha256Digest::new([11; 32])
                && anchor.target() == peritus_app_protocol::WorkbenchReviewTarget::Hunk
    ));
    let receipt = WorkbenchReceipt::new(
        rebind.operation(),
        rebind.query(),
        rebind.expected_revision() + 1,
        Sha256Digest::new([12; 32]),
    )
    .expect("receipt");
    respond(&mut model, &rebind_request, AppResponsePayload::WorkbenchReceipt(receipt));
    assert!(
        model
            .product
            .as_ref()
            .expect("product")
            .review
            .message
            .contains("Read-only explanation started.")
    );
}

#[test]
fn refreshed_review_never_silently_retargets_an_open_draft() {
    let (mut model, query, run) = review_model();
    model.view = View::Diff;
    let requested = WorkbenchReviewQuery::new(query, run, 0, 0);
    model.accept_review_page(requested, page(query, run, 7, 10, "old", "new", &[]));
    key(&mut model, KeyCode::Char('e'));
    let editor = model.editor.as_mut().unwrap();
    editor.buffer = "Explain this exact change".to_owned();
    editor.cursor = editor.buffer.len();
    model.accept_review_page(requested, page(query, run, 8, 11, "new", "newer", &[]));
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert_eq!(model.editor.as_ref().unwrap().buffer, "Explain this exact change");
    let refresh = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('f'),
        KeyModifiers::CONTROL,
    ))));
    let refresh = request(&refresh);
    respond(
        &mut model,
        &refresh,
        AppResponsePayload::WorkbenchReview(page(query, run, 8, 11, "new", "newer", &[])),
    );
    model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('b'),
        KeyModifiers::CONTROL,
    ))));
    let submit = request(&key(&mut model, KeyCode::Enter));
    assert!(matches!(submit.payload(), AppRequestPayload::WorkbenchCommand(command)
        if matches!(command.intent(), WorkbenchIntent::AddReview { anchor, .. }
            if anchor.candidate_digest() == Sha256Digest::new([11; 32]))));
}

#[test]
fn late_review_rejection_preserves_a_newer_editor() {
    let (mut model, query, run) = review_model();
    model.view = View::Diff;
    model.accept_review_page(
        WorkbenchReviewQuery::new(query, run, 0, 0),
        page(query, run, 7, 10, "old", "new", &[]),
    );
    key(&mut model, KeyCode::Char('e'));
    let editor = model.editor.as_mut().unwrap();
    editor.buffer = "First review comment".to_owned();
    editor.cursor = editor.buffer.len();
    let submit = request(&key(&mut model, KeyCode::Enter));
    key(&mut model, KeyCode::Char('v'));
    let editor = model.editor.as_mut().unwrap();
    editor.buffer = "Newer draft in progress".to_owned();
    editor.cursor = editor.buffer.len();
    respond(
        &mut model,
        &submit,
        AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
            AppErrorCode::StaleRevision,
            None,
        )),
    );
    assert_eq!(model.editor.as_ref().unwrap().buffer, "Newer draft in progress");
    key(&mut model, KeyCode::Esc);
    model.accept_review_page(
        WorkbenchReviewQuery::new(query, run, 0, 0),
        page(query, run, 8, 11, "new", "newer", &[]),
    );
    key(&mut model, KeyCode::Char('e'));
    assert_eq!(model.editor.as_ref().unwrap().buffer, "First review comment");
    assert!(key(&mut model, KeyCode::Enter).is_empty());
}

#[test]
fn uncertain_review_request_keeps_one_operation_until_receipt_resolution() {
    let (mut model, query, run) = review_model();
    model.view = View::Diff;
    model.accept_review_page(
        WorkbenchReviewQuery::new(query, run, 0, 0),
        page(query, run, 7, 10, "old", "new", &[]),
    );
    key(&mut model, KeyCode::Char('e'));
    let editor = model.editor.as_mut().unwrap();
    editor.buffer = "Explain this change".to_owned();
    editor.cursor = editor.buffer.len();
    let submit = request(&key(&mut model, KeyCode::Enter));
    respond(
        &mut model,
        &submit,
        AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
            AppErrorCode::Backpressure,
            None,
        )),
    );
    assert!(model.editor.is_none());
    let (command, draft) = model.chat.workbench.unresolved.as_ref().unwrap();
    assert_eq!(submit.payload(), &AppRequestPayload::WorkbenchCommand(command.clone()));
    assert_eq!(draft, "Explain this change");
}
