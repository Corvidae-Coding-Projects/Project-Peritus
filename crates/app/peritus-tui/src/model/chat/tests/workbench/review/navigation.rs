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

#[test]
fn review_editor_targets_the_file_in_the_displayed_diff_page() {
    let (mut model, query, run) = review_model();
    let candidate = Sha256Digest::new([44; 32]);
    let raw = concat!(
        "diff --git a/src/first.rs b/src/first.rs\n",
        "--- a/src/first.rs\n+++ b/src/first.rs\n@@ -1 +1 @@\n-old\n+new\n",
        "diff --git a/src/second.rs b/src/second.rs\n",
        "--- a/src/second.rs\n+++ b/src/second.rs\n@@ -1 +1 @@\n-before\n+after\n"
    );
    let (diff, files) =
        parse_workbench_diff(run, query.workspace(), candidate, raw).expect("two-file diff");
    let review_page = WorkbenchReviewPage::new(
        WorkbenchReviewQuery::new(query, run, 12, 0),
        candidate,
        diff,
        files,
        Vec::new(),
        0,
        vec![
            WorkbenchReviewEvidence::new(
                WorkbenchReviewEvidenceKind::Checks,
                WorkbenchReviewEvidenceState::Missing,
                None,
                None,
                None,
            )
            .expect("evidence"),
        ],
    )
    .expect("review page");
    let second = peritus_app_protocol::WorkbenchReviewDiffPage::project(
        peritus_app_protocol::WorkbenchReviewDiffQuery::new(query, run, 12, 1, 0, 0),
        candidate,
        review_page.diff_digest(),
        review_page.files(),
    )
    .expect("second file page");
    let anchor = second.file_anchor().clone();
    model.chat.workbench.selected = Some(query);
    model.product.as_mut().unwrap().review.page = Some(review_page);
    model.product.as_mut().unwrap().review.diff_page = Some(second);

    model.view = View::Diff;
    key(&mut model, KeyCode::Char('e'));
    model.editor.as_mut().expect("review editor").buffer = "Explain this file".to_owned();
    let effects = key(&mut model, KeyCode::Enter);
    let command = request(&effects);
    let AppRequestPayload::WorkbenchCommand(command) = command.payload() else {
        panic!("review command")
    };
    let WorkbenchIntent::AddReview { anchor: actual, .. } = command.intent() else {
        panic!("review anchor")
    };
    assert_eq!(actual, &anchor);
    assert_eq!(actual.path(), "src/second.rs");
}

#[test]
fn repeated_diff_navigation_during_a_pending_page_request_does_not_mutate_history() {
    let (mut model, query, run) = review_model();
    model.view = View::Diff;
    let current = page(query, run, 13, 20, "old", &"new\n".repeat(40), &[]);
    let candidate = current.candidate_digest();
    let diff = current.diff_digest();
    let files = current.files().to_vec();
    let cursor = peritus_app_protocol::WorkbenchReviewDiffQuery::new(query, run, 13, 0, 0, 0);
    let bounded =
        peritus_app_protocol::WorkbenchReviewDiffPage::project(cursor, candidate, diff, &files)
            .expect("bounded first page");
    assert!(bounded.next().is_some());
    {
        let review = &mut model.product.as_mut().unwrap().review;
        review.page = Some(current);
        review.diff_page = Some(bounded);
    }

    let next = key(&mut model, KeyCode::Char('n'));
    assert!(matches!(request(&next).payload(), AppRequestPayload::QueryWorkbenchReviewDiff(_)));
    let review = &model.product.as_ref().unwrap().review;
    assert_eq!(review.diff_history.len(), 1);
    assert!(review.pending_diff.is_some());

    assert!(key(&mut model, KeyCode::Char('n')).is_empty());
    assert!(key(&mut model, KeyCode::Char('p')).is_empty());
    let review = &model.product.as_ref().unwrap().review;
    assert_eq!(review.diff_history.len(), 1);
    assert!(review.pending_diff.is_some());
}
