//! Hardened caller-rooted Git repository behavior and cleanup tests.

#[cfg(unix)]
use peritus_test_support::FixtureSymlinkKind;
use peritus_test_support::{FixturePath, TempRepositoryErrorKind, TemporaryRepositoryBuilder};
use std::path::PathBuf;

fn repository_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("peritus-test-repository-{}-{label}", std::process::id()))
}

#[test]
fn identical_repositories_make_identical_commits_and_close_cleanly() {
    let first_root = repository_root("deterministic-one");
    let second_root = repository_root("deterministic-two");
    let mut first = TemporaryRepositoryBuilder::new(&first_root)
        .build()
        .expect("first repository must initialize");
    let mut second = TemporaryRepositoryBuilder::new(&second_root)
        .build()
        .expect("second repository must initialize");
    let path = FixturePath::new("src/main.rs").expect("portable path");
    first
        .write_text(&path, "fn main() { println!(\"fixture\"); }\n")
        .expect("first write must work");
    second
        .write_text(&path, "fn main() { println!(\"fixture\"); }\n")
        .expect("second write must work");
    let first_id = first.commit_all("deterministic commit").expect("first commit");
    let second_id = second.commit_all("deterministic commit").expect("second commit");
    assert_eq!(first_id, second_id);
    assert!(matches!(first_id.as_str().len(), 40 | 64));

    first.close().expect("first cleanup must be reported");
    second.close().expect("second cleanup must be reported");
    assert!(!first_root.exists());
    assert!(!second_root.exists());
}

#[test]
fn nonzero_git_exit_is_observation_until_success_is_required() {
    let root = repository_root("git-failure");
    let repository =
        TemporaryRepositoryBuilder::new(&root).build().expect("repository must initialize");
    let output = repository
        .run_git(["rev-parse", "--verify", "does-not-exist"])
        .expect("nonzero exit is still an observation");
    assert!(!output.success());
    assert!(!output.stderr().is_empty());

    let error = repository
        .git_success(["rev-parse", "--verify", "does-not-exist"])
        .expect_err("success helper must classify nonzero exit");
    assert_eq!(error.kind(), TempRepositoryErrorKind::GitFailed);
    assert!(!error.output().expect("failed output must be retained").success());
    repository.close().expect("cleanup must work");
}

#[test]
fn bare_repository_rejects_worktree_mutation() {
    let root = repository_root("bare");
    let mut repository = TemporaryRepositoryBuilder::new(&root)
        .bare(true)
        .build()
        .expect("bare repository must initialize");
    let path = FixturePath::new("payload.bin").expect("portable path");
    let error = repository.write(&path, b"payload").expect_err("bare repository has no worktree");
    assert_eq!(error.kind(), TempRepositoryErrorKind::BareRepository);
    repository.close().expect("cleanup must work");
}

#[test]
fn missing_git_is_typed_and_partially_created_root_is_cleaned() {
    let root = repository_root("missing-git");
    let error = TemporaryRepositoryBuilder::new(&root)
        .git_program("peritus-test-definitely-missing-git")
        .build()
        .expect_err("missing executable must fail");
    assert_eq!(error.kind(), TempRepositoryErrorKind::GitSpawn);
    assert!(!root.exists(), "failed builder must guard and clean its owned root");
}

#[test]
fn broad_or_preexisting_roots_are_rejected_without_mutation() {
    let broad = std::env::temp_dir().join("not-owned-by-peritus");
    let error =
        TemporaryRepositoryBuilder::new(&broad).build().expect_err("unguarded name must fail");
    assert_eq!(error.kind(), TempRepositoryErrorKind::InvalidRoot);
    assert!(!broad.exists());

    let root = repository_root("preexisting");
    std::fs::create_dir(&root).expect("preexisting directory must be created");
    let error = TemporaryRepositoryBuilder::new(&root)
        .build()
        .expect_err("existing path must not be adopted");
    assert_eq!(error.kind(), TempRepositoryErrorKind::InvalidRoot);
    std::fs::remove_dir(&root).expect("test-created empty directory must be removed");
}

#[cfg(unix)]
#[test]
fn writes_never_follow_explicit_adversarial_symlinks() {
    let root = repository_root("symlink");
    let mut repository =
        TemporaryRepositoryBuilder::new(&root).build().expect("repository must initialize");
    let link = FixturePath::new("escape").expect("portable link path");
    repository
        .create_adversarial_symlink(&link, "../outside", FixtureSymlinkKind::Directory)
        .expect("adversarial symlink must be created");
    let escaped = FixturePath::new("escape/payload.bin").expect("portable nested path");
    let error = repository
        .write(&escaped, b"must-not-escape")
        .expect_err("write through symlink must fail");
    assert_eq!(error.kind(), TempRepositoryErrorKind::UnsafePath);
    repository.close().expect("cleanup must not follow symlink");
}
