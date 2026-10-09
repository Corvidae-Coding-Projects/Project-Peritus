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
fn negotiated_summary_page_uses_bounded_hunk_for_small_area_scrolling() {
    let (mut model, query, run) = review_model();
    model.view = View::Diff;
    let candidate = Sha256Digest::new([81; 32]);
    let mut raw = String::from(
        "diff --git a/src/large.rs b/src/large.rs\n--- a/src/large.rs\n+++ b/src/large.rs\n@@ -0,0 +1,40 @@\n",
    );
    for _ in 0..40 {
        raw.push('+');
        raw.push_str(&"wrapped line ".repeat(10));
        raw.push('\n');
    }
    let diff_page = peritus_app_protocol::parse_workbench_diff_page(
        peritus_app_protocol::WorkbenchReviewDiffQuery::new(query, run, 18, 0, 0, 0),
        candidate,
        &raw,
    )
    .expect("bounded diff page");
    let digest = diff_page.diff_digest();
    let summary_page = WorkbenchReviewPage::new(
        WorkbenchReviewQuery::new(query, run, 18, 0),
        candidate,
        digest,
        Vec::new(),
        Vec::new(),
        0,
        Vec::new(),
    )
    .expect("summary-backed review page");
    let review = &mut model.product.as_mut().unwrap().review;
    review.page = Some(summary_page);
    review.diff_page = Some(diff_page);
    model.chat.viewport = Some(Rect::new(0, 0, 56, 16));

    key(&mut model, KeyCode::End);
    let preview_scroll = model.product.as_ref().unwrap().review.scroll;
    assert!(preview_scroll > 0);
    let line = &model.product.as_ref().unwrap().review.diff_page.as_ref().unwrap().lines()[0];
    model.product.as_mut().unwrap().review.raw_line = Some((line.raw_offset(), vec![b'x'; 32_768]));
    key(&mut model, KeyCode::End);
    assert!(model.product.as_ref().unwrap().review.scroll > preview_scroll);
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

#[test]
fn structured_summary_raw_toggle_fetches_exact_review_bytes_and_pages_them() {
    let (mut model, query, run) = review_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchReviewSummary)
            .expect("summary feature"),
    );
    model.chat.buffer = "/diff".to_owned();
    let summary_request = request(&model.refresh_review());
    let raw = raw_review_stream_fixture();
    let (summary, candidate, diff_digest) = review_summary_fixture(query, run, &raw);
    let effects =
        respond(&mut model, &summary_request, AppResponsePayload::WorkbenchReviewSummary(summary));
    let diff_request = request(&effects);
    let AppRequestPayload::QueryWorkbenchReviewDiff(cursor) = diff_request.payload() else {
        panic!("summary continuation fetches a bounded diff page")
    };
    let diff_page = peritus_app_protocol::parse_workbench_diff_page(*cursor, candidate, &raw)
        .expect("exact current diff page");
    let effects =
        respond(&mut model, &diff_request, AppResponsePayload::WorkbenchReviewDiff(diff_page));
    assert!(effects.is_empty());

    model.view = View::Diff;
    let first_effects = key(&mut model, KeyCode::Char('t'));
    let first = request(&first_effects);
    let AppRequestPayload::QueryWorkbenchReviewDiffBytes(first_query) = first.payload() else {
        panic!("raw toggle requests summary-bound diff bytes")
    };
    assert_eq!(first_query.offset(), 0);
    assert_eq!(first_query.candidate_digest(), candidate);
    assert_eq!(first_query.diff_digest(), diff_digest);
    assert_eq!(first_query.revision(), 7);
    let total_bytes = u32::try_from(raw.len()).unwrap();
    assert!(total_bytes > first_query.maximum_bytes());
    let first_bytes = peritus_app_protocol::WorkbenchReviewDiffBytes::new(
        *first_query,
        total_bytes,
        raw.as_bytes()[..first_query.maximum_bytes() as usize].to_vec(),
    )
    .expect("first exact raw range");
    respond(&mut model, &first, AppResponsePayload::WorkbenchReviewDiffBytes(first_bytes));
    let raw_line = model.product.as_ref().unwrap().review.raw_line.as_ref().unwrap();
    assert_eq!(raw_line.1.last(), Some(&0xc3));
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).unwrap();
    let text = frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect::<String>();
    assert!(text.contains("fresh-current-line-0000"));
    assert!(!text.contains("+new"), "stale ProductRunSnapshot diff was rendered: {text}");

    let next = request(&key(&mut model, KeyCode::Char(']')));
    let AppRequestPayload::QueryWorkbenchReviewDiffBytes(next_query) = next.payload() else {
        panic!("raw toggle pages the next exact range")
    };
    assert_eq!(next_query.offset(), first_query.maximum_bytes());
    assert_eq!(next_query.candidate_digest(), candidate);
    assert_eq!(next_query.diff_digest(), diff_digest);
    let next_bytes = peritus_app_protocol::WorkbenchReviewDiffBytes::new(
        *next_query,
        total_bytes,
        raw.as_bytes()[next_query.offset() as usize
            ..(next_query.offset() + next_query.maximum_bytes()) as usize]
            .to_vec(),
    )
    .expect("next exact raw range");
    respond(&mut model, &next, AppResponsePayload::WorkbenchReviewDiffBytes(next_bytes));
    let raw_line = model.product.as_ref().unwrap().review.raw_line.as_ref().unwrap();
    assert_eq!(raw_line.1.first(), Some(&0xa9));
    let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).unwrap();
    let text = frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect::<String>();
    assert!(text.contains("fresh-current-line-0000"));
    let previous = request(&key(&mut model, KeyCode::Char('[')));
    let AppRequestPayload::QueryWorkbenchReviewDiffBytes(previous_query) = previous.payload()
    else {
        panic!("raw toggle returns to the previous exact range")
    };
    assert_eq!(previous_query.offset(), 0);

    assert!(key(&mut model, KeyCode::Char('t')).is_empty());
    let review = &model.product.as_ref().unwrap().review;
    assert!(!review.raw_stream);
    assert!(!review.raw);
}

fn raw_review_stream_fixture() -> String {
    use std::fmt::Write as _;

    let mut raw = String::from(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1,400 @@\n-old\n",
    );
    let utf8_lead = 32 * 1024 - 1;
    let mut filler_index = 0;
    while raw.len() + 100 < utf8_lead {
        let prefix = format!("+fresh-current-line-{filler_index:04}-");
        raw.push_str(&prefix);
        raw.push_str(&"x".repeat(98 - prefix.len()));
        raw.push('\n');
        filler_index += 1;
    }
    let padding = utf8_lead - raw.len() - 1;
    raw.push('+');
    raw.push_str(&"x".repeat(padding));
    raw.push('é');
    raw.push('\n');
    for index in 0..400 {
        writeln!(raw, "+fresh-current-line-{index:04}-{}", "x".repeat(96)).unwrap();
    }
    raw
}

fn review_summary_fixture(
    query: WorkbenchQuery,
    run: RunId,
    raw: &str,
) -> (peritus_app_protocol::WorkbenchReviewSummary, Sha256Digest, Sha256Digest) {
    let candidate = Sha256Digest::new([101; 32]);
    let (diff_digest, files) =
        parse_workbench_diff(run, query.workspace(), candidate, raw).expect("review diff");
    let hunks = files.iter().flat_map(peritus_app_protocol::WorkbenchDiffFile::hunks);
    let hunks = hunks.collect::<Vec<_>>();
    let total_hunks = u64::try_from(hunks.len()).unwrap();
    let total_lines = hunks.iter().map(|hunk| u64::try_from(hunk.lines().len()).unwrap()).sum();
    let summary = peritus_app_protocol::WorkbenchReviewSummary::new(
        WorkbenchReviewQuery::new(query, run, 7, 0),
        candidate,
        diff_digest,
        u32::try_from(files.len()).unwrap(),
        total_hunks,
        total_lines,
        true,
        Vec::new(),
        0,
        Vec::new(),
    )
    .expect("summary");
    (summary, candidate, diff_digest)
}
