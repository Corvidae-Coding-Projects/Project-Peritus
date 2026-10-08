//! Immutable history observations ignore shared replacement refs.

mod support;

use peritus_git::{CreateWorktree, HistoryRequest, WorktreeAccess, WorktreeName};
use support::{RepositoryFixture, checked_git};

#[test]
fn shared_history_metadata_cannot_rewrite_immutable_observations() {
    let fixture = RepositoryFixture::sha1();
    let first = checked_git(&fixture.root, &["rev-parse", "HEAD"]);
    checked_git(&fixture.root, &["commit", "--quiet", "--allow-empty", "-m", "middle café ¥"]);
    let middle = checked_git(&fixture.root, &["rev-parse", "HEAD"]);
    checked_git(&fixture.root, &["commit", "--quiet", "--allow-empty", "-m", "latest"]);
    let repository = fixture.open();
    let baseline = repository.resolve_baseline("HEAD").expect("baseline");
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("history-reader").expect("name"),
            fixture.worktree_path("history-reader"),
            baseline,
            WorktreeAccess::ReadOnly,
        ))
        .expect("worktree");
    let request = HistoryRequest::page(&worktree, baseline.commit(), 1, 1).expect("page");
    let before = repository.history(request).expect("original history");
    assert_eq!(before.commits()[0].subject(), "middle café ¥");
    assert_eq!(before.commits()[0].parent_count(), 1);
    for encoding in ["ISO-8859-1", "SHIFT-JIS"] {
        checked_git(&fixture.root, &["config", "i18n.logOutputEncoding", encoding]);
        assert_eq!(repository.history(request).expect("stable UTF-8 subject encoding"), before);
    }
    checked_git(&fixture.root, &["config", "log.showSignature", "true"]);
    checked_git(&fixture.root, &["notes", "add", "-m", "mutable note", &middle]);
    assert_eq!(repository.history(request).expect("stable structured history"), before);
    checked_git(&fixture.root, &["replace", &middle, &first]);
    assert_eq!(checked_git(&fixture.root, &["log", "-1", "--format=%s", &middle]), "baseline");
    let after = repository.history(request).expect("history after shared ref mutation");
    assert_eq!(after, before);
    let git_dir = fixture.root.join(".git");
    std::fs::write(git_dir.join("shallow"), format!("{middle}\n"))
        .expect("shared shallow boundary");
    assert_eq!(repository.history(request).expect("ignore shallow boundary"), before);
    std::fs::remove_file(git_dir.join("shallow")).expect("remove shallow boundary");
    std::fs::write(git_dir.join("info/grafts"), format!("{middle}\n"))
        .expect("shared graft boundary");
    assert_eq!(repository.history(request).expect("ignore graft boundary"), before);
    std::fs::remove_file(git_dir.join("info/grafts")).expect("remove graft boundary");
    assert_eq!(repository.history(request).expect("original ancestry remains"), before);
    let resolved = repository.resolve_baseline(&middle).expect("original middle object");
    assert_eq!(resolved.commit().to_string(), middle);
}
