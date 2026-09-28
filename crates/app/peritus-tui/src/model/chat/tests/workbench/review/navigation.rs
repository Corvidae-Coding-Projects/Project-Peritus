use super::*;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

#[test]
fn raw_diff_tab_navigation_does_not_change_hidden_structured_focus() {
    let (mut model, query, run) = review_model();
    model.view = View::Diff;
    model.accept_review_page(
        WorkbenchReviewQuery::new(query, run, 0, 0),
        page(query, run, 7, 10, "old", "new", &[]),
    );
    key(&mut model, KeyCode::Char('t'));
    key(&mut model, KeyCode::Tab);
    assert_eq!(model.view, View::Review);
}

#[test]
fn structured_hunks_and_long_comments_are_fully_reachable() {
    let (mut model, query, run) = review_model();
    model.view = View::Diff;
    let added = format!("{}HUNK_TAIL", "new\n+".repeat(80));
    let mut current = page(query, run, 7, 10, "old", &added, &[]);
    let id = peritus_app_protocol::ControlOperationId::new([18; 16]).unwrap();
    let comment = WorkbenchReviewComment::new(
        id,
        1,
        current.files()[0].anchor().clone(),
        WorkbenchReviewFeedback::Explain,
        WorkbenchInputText::new(format!("{}COMMENT_TAIL", "long comment words ".repeat(80)))
            .unwrap(),
        WorkbenchInputSelection::new(WorkbenchInputId::new(*id.as_bytes()).unwrap(), 1).unwrap(),
        WorkbenchReviewCommentState::Open,
    )
    .unwrap();
    current = page(query, run, 7, 10, "old", &added, &[comment]);
    model.accept_review_page(WorkbenchReviewQuery::new(query, run, 0, 0), current);
    for (width, height) in [(80, 24), (120, 38)] {
        model.chat.viewport = Some(Rect::new(0, 0, width, height));
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        for (focus, tail) in [
            (crate::model::ReviewFocus::Hunk, "HUNK_TAIL"),
            (crate::model::ReviewFocus::Comment, "COMMENT_TAIL"),
        ] {
            model.product.as_mut().unwrap().review.focus = focus;
            for _ in 0..100 {
                key(&mut model, KeyCode::PageDown);
            }
            let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).unwrap();
            let bottom = frame.buffer.clone();
            let text =
                bottom.content().iter().map(ratatui::buffer::Cell::symbol).collect::<String>();
            assert!(text.contains(tail), "missing {tail} at {width}x{height}: {text}");
            key(&mut model, KeyCode::PageUp);
            let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).unwrap();
            assert_ne!(frame.buffer, &bottom);
        }
    }
}
