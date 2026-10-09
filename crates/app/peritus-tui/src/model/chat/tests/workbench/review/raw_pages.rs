use super::*;

#[test]
fn raw_only_summary_fetches_and_pages_exact_bytes_instead_of_cached_run_diff() {
    let (mut model, query, run) = review_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchReviewSummary)
            .expect("summary feature"),
    );
    model.chat.buffer = "/diff".to_owned();
    let initial = request(&model.refresh_review());
    let mut raw = String::from(
        "diff --git a/../unsafe.rs b/../unsafe.rs\n--- a/../unsafe.rs\n+++ b/../unsafe.rs\n@@ -0,0 +1 @@\n+fresh exact bytes ",
    );
    raw.push_str(&"z".repeat(40_000));
    raw.push('\n');
    let candidate = Sha256Digest::new([91; 32]);
    assert!(
        peritus_app_protocol::parse_workbench_diff_page(
            peritus_app_protocol::WorkbenchReviewDiffQuery::new(query, run, 7, 0, 0, 0),
            candidate,
            &raw,
        )
        .is_err()
    );
    let diff_digest = peritus_codec::sha256(raw.as_bytes());
    let summary = peritus_app_protocol::WorkbenchReviewSummary::new(
        WorkbenchReviewQuery::new(query, run, 7, 0),
        candidate,
        diff_digest,
        0,
        0,
        0,
        false,
        Vec::new(),
        0,
        Vec::new(),
    )
    .expect("raw-only summary");
    let bytes_effects =
        respond(&mut model, &initial, AppResponsePayload::WorkbenchReviewSummary(summary));
    let first = request(&bytes_effects);
    let AppRequestPayload::QueryWorkbenchReviewDiffBytes(first_query) = first.payload() else {
        panic!("raw-only summary requests exact bytes")
    };
    assert_eq!(first_query.offset(), 0);
    assert_eq!(first_query.diff_digest(), diff_digest);
    assert!(model.product.as_ref().unwrap().review.raw_stream);
    let first_bytes = peritus_app_protocol::WorkbenchReviewDiffBytes::new(
        *first_query,
        u32::try_from(raw.len()).unwrap(),
        raw.as_bytes()[..32 * 1024].to_vec(),
    )
    .expect("first raw range");
    respond(&mut model, &first, AppResponsePayload::WorkbenchReviewDiffBytes(first_bytes));
    model.view = View::Diff;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 24)).unwrap();
    let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).unwrap();
    let text = frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect::<String>();
    assert!(text.contains("fresh exact bytes"));
    assert!(!text.contains("old"));

    let second = request(&key(&mut model, KeyCode::Char(']')));
    let AppRequestPayload::QueryWorkbenchReviewDiffBytes(second_query) = second.payload() else {
        panic!("raw navigation requests the next range")
    };
    assert_eq!(second_query.offset(), 32 * 1024);
    assert_eq!(second_query.candidate_digest(), candidate);
    assert_eq!(second_query.diff_digest(), diff_digest);
    let last_bytes = peritus_app_protocol::WorkbenchReviewDiffBytes::new(
        *second_query,
        u32::try_from(raw.len()).unwrap(),
        raw.as_bytes()[32 * 1024..].to_vec(),
    )
    .expect("last raw range");
    respond(&mut model, &second, AppResponsePayload::WorkbenchReviewDiffBytes(last_bytes));
    let previous = request(&key(&mut model, KeyCode::Char('[')));
    let AppRequestPayload::QueryWorkbenchReviewDiffBytes(previous_query) = previous.payload()
    else {
        panic!("raw navigation returns to the prior range")
    };
    assert_eq!(previous_query.offset(), 0);
}

#[test]
fn truncated_review_line_requests_and_retains_the_exact_safe_raw_range() {
    let (mut model, query, run) = review_model();
    model.chat.buffer = "/diff".to_owned();
    let first = request(&key(&mut model, KeyCode::Enter));
    let long = "x".repeat(5_000);
    let legacy = page(query, run, 8, 11, &long, &long, &[]);
    let candidate = legacy.candidate_digest();
    let diff = legacy.diff_digest();
    let files = legacy.files().to_vec();
    let effects = respond(&mut model, &first, AppResponsePayload::WorkbenchReview(legacy));
    let page_request = request(&effects);
    let AppRequestPayload::QueryWorkbenchReviewDiff(cursor) = page_request.payload() else {
        panic!("diff page request")
    };
    let bounded =
        peritus_app_protocol::WorkbenchReviewDiffPage::project(*cursor, candidate, diff, &files)
            .expect("bounded page");
    let effects =
        respond(&mut model, &page_request, AppResponsePayload::WorkbenchReviewDiff(bounded));
    let bytes_request = request(&effects);
    let AppRequestPayload::QueryWorkbenchReviewDiffBytes(raw_query) = bytes_request.payload()
    else {
        panic!("raw range request")
    };
    let raw = peritus_app_protocol::WorkbenchReviewDiffBytes::new(
        *raw_query,
        raw_query.offset() + raw_query.maximum_bytes(),
        b"verified source range".to_vec(),
    )
    .expect("raw range");
    respond(&mut model, &bytes_request, AppResponsePayload::WorkbenchReviewDiffBytes(raw));
    let review = &model.product.as_ref().expect("product").review;
    assert_eq!(review.raw_line.as_ref().map(|(offset, _)| *offset), Some(raw_query.offset()));
}

#[test]
fn expanding_control_preview_still_requests_exact_raw_line_bytes() {
    let (mut model, query, run) = review_model();
    model.chat.buffer = "/diff".to_owned();
    let first = request(&key(&mut model, KeyCode::Enter));
    let controls = "\u{1b}".repeat(500);
    let legacy = page(query, run, 8, 11, &controls, &controls, &[]);
    let candidate = legacy.candidate_digest();
    let diff = legacy.diff_digest();
    let files = legacy.files().to_vec();
    let effects = respond(&mut model, &first, AppResponsePayload::WorkbenchReview(legacy));
    let page_request = request(&effects);
    let AppRequestPayload::QueryWorkbenchReviewDiff(cursor) = page_request.payload() else {
        panic!("diff page request")
    };
    let bounded =
        peritus_app_protocol::WorkbenchReviewDiffPage::project(*cursor, candidate, diff, &files)
            .expect("bounded page");
    assert!(bounded.lines()[0].is_truncated());
    let effects =
        respond(&mut model, &page_request, AppResponsePayload::WorkbenchReviewDiff(bounded));
    let bytes_request = request(&effects);
    let AppRequestPayload::QueryWorkbenchReviewDiffBytes(raw_query) = bytes_request.payload()
    else {
        panic!("exact raw-line request")
    };
    assert_eq!(raw_query.maximum_bytes() as usize, controls.len());
}

#[test]
fn raw_review_bytes_page_across_large_lines_and_navigate_back_by_line() {
    let (mut model, query, run) = review_model();
    model.chat.buffer = "/diff".to_owned();
    let first = request(&key(&mut model, KeyCode::Enter));
    let long = "x".repeat(70_000);
    let legacy = page(query, run, 9, 12, &long, &long, &[]);
    let candidate = legacy.candidate_digest();
    let diff = legacy.diff_digest();
    let files = legacy.files().to_vec();
    let effects = respond(&mut model, &first, AppResponsePayload::WorkbenchReview(legacy));
    let page_request = request(&effects);
    let AppRequestPayload::QueryWorkbenchReviewDiff(cursor) = page_request.payload() else {
        panic!("diff page request")
    };
    let bounded =
        peritus_app_protocol::WorkbenchReviewDiffPage::project(*cursor, candidate, diff, &files)
            .expect("bounded page");
    let effects =
        respond(&mut model, &page_request, AppResponsePayload::WorkbenchReviewDiff(bounded));
    let mut bytes_request = request(&effects);
    let AppRequestPayload::QueryWorkbenchReviewDiffBytes(mut raw_query) =
        bytes_request.payload().clone()
    else {
        panic!("first raw range request")
    };
    let lines = model.product.as_ref().unwrap().review.raw_lines.clone();
    assert!(lines.len() >= 2);
    assert!(lines[0].1 > 32 * 1024);
    assert!(lines[1].1 > 32 * 1024);

    let first_line_start = lines[0].0;
    let first_line_end = first_line_start + lines[0].1;
    assert_eq!(raw_query.offset(), first_line_start);
    assert!(key(&mut model, KeyCode::Char(']')).is_empty());
    assert_eq!(model.product.as_ref().unwrap().review.raw_index, 0);
    while raw_query.offset() + raw_query.maximum_bytes() < first_line_end {
        let length = raw_query.maximum_bytes();
        let raw = peritus_app_protocol::WorkbenchReviewDiffBytes::new(
            raw_query,
            first_line_end,
            vec![b'x'; length as usize],
        )
        .expect("first raw chunk");
        let _ =
            respond(&mut model, &bytes_request, AppResponsePayload::WorkbenchReviewDiffBytes(raw));
        bytes_request = request(&key(&mut model, KeyCode::Char(']')));
        let AppRequestPayload::QueryWorkbenchReviewDiffBytes(next) = bytes_request.payload() else {
            panic!("continued raw range request")
        };
        raw_query = *next;
    }
    let remaining = first_line_end - raw_query.offset();
    let raw = peritus_app_protocol::WorkbenchReviewDiffBytes::new(
        raw_query,
        first_line_end,
        vec![b'x'; remaining as usize],
    )
    .expect("final first-line chunk");
    let _ = respond(&mut model, &bytes_request, AppResponsePayload::WorkbenchReviewDiffBytes(raw));

    bytes_request = request(&key(&mut model, KeyCode::Char(']')));
    let AppRequestPayload::QueryWorkbenchReviewDiffBytes(second_line) = bytes_request.payload()
    else {
        panic!("next truncated line request")
    };
    assert_eq!(second_line.offset(), lines[1].0);
    let raw = peritus_app_protocol::WorkbenchReviewDiffBytes::new(
        *second_line,
        second_line.offset() + second_line.maximum_bytes(),
        vec![b'y'; second_line.maximum_bytes() as usize],
    )
    .expect("second-line chunk");
    let _ = respond(&mut model, &bytes_request, AppResponsePayload::WorkbenchReviewDiffBytes(raw));

    bytes_request = request(&key(&mut model, KeyCode::Char('[')));
    let AppRequestPayload::QueryWorkbenchReviewDiffBytes(previous_line) = bytes_request.payload()
    else {
        panic!("previous truncated line request")
    };
    assert_eq!(previous_line.offset(), first_line_end - 32 * 1024);
}
