//! Real immutable-worktree structured Git tool tests.

mod support;

use peritus_tools_git::{
    DiffInput, GitReadService, HistoryInput, RenderedOutput, StatusInput, descriptor_catalog,
    descriptor_digest,
};

#[test]
fn structured_status_diff_history_and_snapshot_use_real_git_observations() {
    let fixture = support::git_fixture_with("git-read", |_| {});
    let service = GitReadService::new(&fixture.workspace);

    let source_error =
        fixture.workspace.git_diff("missing-reference", 1, 1).expect_err("missing revision");
    let tool_error = service
        .diff(&DiffInput::new("missing-reference".to_owned(), 1, 1).expect("input"))
        .expect_err("missing revision");
    assert_eq!(tool_error.code(), source_error.kind().code());
    assert_eq!(tool_error.detail(), source_error.detail());
    assert!(tool_error.to_string().contains(source_error.kind().code()));

    let status = service.status(StatusInput).expect("status");
    assert!(status.is_clean());
    assert!(
        RenderedOutput::status(&status)
            .expect("status render")
            .structured()
            .canonical_bytes()
            .len()
            > 32
    );

    let diff = service
        .diff(&DiffInput::new(fixture.first_commit.clone(), 100, 64 * 1024).expect("diff input"))
        .expect("diff");
    assert_eq!(diff.entries().len(), 2);
    assert_eq!(
        diff.entries().iter().map(peritus_git::DiffEntry::path).collect::<Vec<_>>(),
        ["README.md", "src/main.rs"]
    );
    assert!(!diff.patch().is_empty());
    assert!(!RenderedOutput::diff(&diff).expect("diff render").truncated());

    let history = service.history(HistoryInput::new(10).expect("history input")).expect("history");
    assert_eq!(history.commits().len(), 2);
    assert_eq!(history.commits()[0].subject(), "second");
    assert_eq!(history.commits()[1].subject(), "first");
    assert!(!RenderedOutput::history(&history).expect("history render").truncated());
    let caller_budget = RenderedOutput::history_with_budget(&history, 1_024)
        .expect("history render respects caller budget");
    assert!(caller_budget.structured().canonical_bytes().len() <= 1_024);
    let first_page =
        service.history(HistoryInput::page(1, 0).expect("first history page")).expect("first page");
    assert_eq!(first_page.commits().len(), 1);
    assert_eq!(first_page.next_offset(), Some(1));
    let second_page = service
        .history(
            HistoryInput::page(1, first_page.next_offset().expect("more history"))
                .expect("second history page"),
        )
        .expect("second page");
    assert_eq!(second_page.commits().len(), 1);
    assert_eq!(second_page.commits()[0].subject(), "first");
    assert_eq!(second_page.next_offset(), None);
    let parent_page = service
        .history(HistoryInput::page(1, 0).expect("parent page").with_parent_offset(1))
        .expect("parent continuation");
    assert_eq!(parent_page.commits()[0].parent_offset(), 1);
    assert_eq!(parent_page.commits()[0].parent_count(), 1);
    assert!(parent_page.commits()[0].parents().is_empty());
    assert_eq!(parent_page.commits()[0].next_parent_offset(), None);
    let rendered = RenderedOutput::history(&first_page).expect("page render");
    assert!(rendered.truncated());

    let snapshot = service.current_snapshot();
    assert_eq!(Some(snapshot.commit()), status.head());
    assert_eq!(snapshot.workspace_id(), fixture.workspace.snapshot().workspace_id());
    assert!(!RenderedOutput::snapshot(&snapshot).expect("snapshot render").truncated());
}

#[test]
fn descriptor_catalog_is_complete_canonical_and_deterministic() {
    let first = descriptor_catalog().expect("catalog");
    let second = descriptor_catalog().expect("catalog");
    assert_eq!(first.len(), 6);
    assert_eq!(
        first.iter().map(|value| value.name().as_str()).collect::<Vec<_>>(),
        ["git.candidate", "git.diff", "git.history", "git.rollback", "git.snapshot", "git.status",]
    );
    assert_eq!(
        first
            .iter()
            .map(peritus_tool_protocol::ToolDescriptor::canonical_bytes)
            .collect::<Vec<_>>(),
        second
            .iter()
            .map(peritus_tool_protocol::ToolDescriptor::canonical_bytes)
            .collect::<Vec<_>>()
    );
    assert_eq!(descriptor_digest().expect("digest"), descriptor_digest().expect("digest"));
}

#[test]
fn history_subject_bytes_continue_without_skipping_the_commit() {
    let fixture = support::git_fixture_with("git-history-subject", |source| {
        source
            .write_text(
                &peritus_test_support::FixturePath::new("README.md").expect("path"),
                "middle\n",
            )
            .expect("middle README");
        source.commit_all(&format!("subject {}", "x".repeat(5_000))).expect("long subject commit");
        source
            .write_text(
                &peritus_test_support::FixturePath::new("src/final.rs").expect("path"),
                "final\n",
            )
            .expect("final source");
    });
    let service = GitReadService::new(&fixture.workspace);
    let full =
        service.history(HistoryInput::new(10).expect("full history input")).expect("history");
    let multi = RenderedOutput::history(&full).expect("multi-row history");
    let multi_bytes = multi.structured().canonical_bytes();
    let multi_json = String::from_utf8_lossy(multi_bytes);
    assert!(multi_json.contains("\"next_offset\":1"));
    assert!(multi_json.contains("\"next_subject_offset\":64"));
    let budgeted = RenderedOutput::history_with_budget(&full, 1_024)
        .expect("budget retains the complete first commit row");
    assert!(budgeted.structured().canonical_bytes().len() <= 1_024);
    assert!(
        String::from_utf8_lossy(budgeted.structured().canonical_bytes())
            .contains("\"next_subject_offset\":null")
    );
    let first = service
        .history(HistoryInput::page(1, 1).expect("history page"))
        .expect("first subject range");
    let rendered = RenderedOutput::history(&first).expect("first subject projection");
    assert!(
        rendered
            .structured()
            .canonical_bytes()
            .windows(19)
            .any(|window| window == b"next_subject_offset")
    );
    assert!(
        rendered
            .structured()
            .canonical_bytes()
            .windows(14)
            .any(|window| window == b"subject_offset")
    );

    let next = service
        .history(HistoryInput::page(1, 1).expect("same commit page").with_subject_offset(64))
        .expect("continued subject range");
    assert_eq!(first.commits()[0].subject().len(), 5_008);
    assert_eq!(next.offset(), first.offset());
    assert_eq!(next.commits()[0].commit(), first.commits()[0].commit());
    let tail = service
        .history(HistoryInput::page(1, 1).expect("tail page").with_subject_offset(4_992))
        .expect("subject bytes beyond former limit");
    let tail_output = RenderedOutput::history(&tail).expect("tail render");
    assert!(
        String::from_utf8_lossy(tail_output.structured().canonical_bytes())
            .contains("\"next_subject_offset\":null")
    );
    let continued = RenderedOutput::history(&next).expect("continued subject projection");
    assert!(continued.structured().canonical_bytes().windows(2).any(|window| window == b"xx"));
}

#[test]
fn last_subject_fragment_has_no_false_continuation() {
    let long_subject = format!("oldest {}", "z".repeat(500));
    let fixture =
        support::git_fixture_with_first_subject("git-history-last-subject", &long_subject, |_| {});
    let service = GitReadService::new(&fixture.workspace);
    let last = service
        .history(HistoryInput::page(1, 1).expect("last commit page").with_subject_offset(448))
        .expect("last subject fragment");
    assert_eq!(last.next_offset(), None);
    let output = RenderedOutput::history(&last).expect("last fragment render");
    assert!(!output.truncated());
    let json = String::from_utf8_lossy(output.structured().canonical_bytes());
    assert!(json.contains("\"next_subject_offset\":null"));
}
