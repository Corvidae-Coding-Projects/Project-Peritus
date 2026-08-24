//! Real immutable-worktree structured Git tool tests.

mod support;

use peritus_tools_git::{
    DiffInput, GitReadService, HistoryInput, RenderedOutput, StatusInput, descriptor_catalog,
    descriptor_digest,
};

#[test]
fn structured_status_diff_history_and_snapshot_use_real_git_observations() {
    let fixture = support::git_fixture("git-read");
    let service = GitReadService::new(&fixture.workspace);

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

    let snapshot = service.current_snapshot();
    assert_eq!(snapshot.commit(), status.head());
    assert_eq!(snapshot.workspace_id(), fixture.workspace.snapshot().workspace_id());
    assert!(!RenderedOutput::snapshot(&snapshot).expect("snapshot render").truncated());
}

#[test]
fn merge_is_typed_unsupported_and_never_mutates_refs() {
    let fixture = support::git_fixture("git-merge-unsupported");
    let before = fixture.source.git_success(["show-ref"]).expect("refs before").stdout().to_vec();
    let service = GitReadService::new(&fixture.workspace);
    let error = service.merge_unsupported().expect_err("merge remains unsupported");
    assert_eq!(error.kind(), peritus_tools_git::GitToolErrorKind::Unsupported);
    let after = fixture.source.git_success(["show-ref"]).expect("refs after").stdout().to_vec();
    assert_eq!(before, after);
}

#[test]
fn descriptor_catalog_is_complete_canonical_and_deterministic() {
    let first = descriptor_catalog().expect("catalog");
    let second = descriptor_catalog().expect("catalog");
    assert_eq!(first.len(), 7);
    assert_eq!(
        first.iter().map(|value| value.name().as_str()).collect::<Vec<_>>(),
        [
            "git.candidate",
            "git.diff",
            "git.history",
            "git.merge",
            "git.rollback",
            "git.snapshot",
            "git.status",
        ]
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
