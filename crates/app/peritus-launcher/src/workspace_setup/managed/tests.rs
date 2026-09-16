use std::{path::PathBuf, process::Command};

use super::*;
use crate::ProductBootstrap;

#[test]
fn trust_creates_registered_managed_copy_and_reports_dirty_state() {
    let temporary = tempfile::tempdir().expect("temporary root");
    let source = initialized_repository(temporary.path());
    let nested = source.join("src/nested");
    fs::create_dir_all(&nested).expect("nested directory");
    let repository = DiscoveredRepository::open(&nested).expect("descendant discovery");
    assert_eq!(
        repository.repository().expect("Git repository").identity().repository_root(),
        source
    );
    let profile = new_profile(&repository).expect("restricted profile");
    assert_eq!(health(&profile), WorkspaceHealth::Restricted);

    let layout = AppLayout::for_test(&temporary.path().join("application"))
        .prepare()
        .expect("application layout");
    let trusted = trust(&layout, &repository, profile).expect("trusted workspace");
    assert_eq!(health(&trusted), WorkspaceHealth::Ready);
    assert!(Path::new(trusted.registration_file().expect("registration")).is_file());
    assert_ne!(trusted.managed_root(), Some(source.to_str().expect("source path")));

    let managed_file = Path::new(trusted.managed_root().expect("managed root")).join("file.txt");
    fs::write(managed_file, "changed\n").expect("modify managed copy");
    assert_eq!(health(&trusted), WorkspaceHealth::Dirty);
}

#[test]
fn interrupted_registration_recovers_and_removed_recent_keeps_daemon_catalog_valid() {
    let temporary = tempfile::tempdir().expect("temporary root");
    let source = initialized_repository(temporary.path());
    let repository = DiscoveredRepository::open(&source).expect("repository");
    let layout = AppLayout::for_test(&temporary.path().join("application"))
        .prepare()
        .expect("application layout");
    let trusted = trust(&layout, &repository, new_profile(&repository).expect("profile"))
        .expect("trusted workspace");
    fs::remove_file(trusted.registration_file().expect("registration"))
        .expect("simulate interrupted registration publication");
    let recovered = trust(&layout, &repository, trusted.restrict()).expect("recovery");
    assert_eq!(health(&recovered), WorkspaceHealth::Ready);
    fs::write(recovered.registration_file().expect("registration"), b"interrupted")
        .expect("simulate interrupted repair");
    let repaired = trust(&layout, &repository, recovered).expect("repair registration");
    assert_eq!(health(&repaired), WorkspaceHealth::Ready);

    let configured = ProductBootstrap::new(layout.clone())
        .configure_workspace(repaired.clone())
        .expect("configure workspace");
    assert_eq!(configured.daemon_config().projects().len(), 1);
    assert_eq!(configured.daemon_config().workspaces().len(), 1);
    assert!(!configured.daemon_config().tools().allowed().is_empty());

    let forgotten = ProductBootstrap::new(layout)
        .remove_workspace(repaired.workspace_id())
        .expect("forget recent workspace");
    assert!(forgotten.state().workspaces().recent().is_empty());
    assert_eq!(forgotten.daemon_config().workspaces().len(), 1);
    assert!(forgotten.daemon_config().tools().allowed().is_empty());
}

#[test]
fn trusted_repair_adopts_advanced_detached_head_and_preserves_unfinished_files() {
    let temporary = tempfile::tempdir().expect("temporary root");
    let source = initialized_repository(temporary.path());
    let repository = DiscoveredRepository::open(&source).expect("repository");
    let layout = AppLayout::for_test(&temporary.path().join("application"))
        .prepare()
        .expect("application layout");
    let trusted = trust(&layout, &repository, new_profile(&repository).expect("profile"))
        .expect("trusted workspace");
    let managed = PathBuf::from(trusted.managed_root().expect("managed root"));
    fs::write(managed.join("file.txt"), "agent result\n").expect("modify tracked file");
    git(&managed, &["add", "--", "file.txt"]);
    git(&managed, &["commit", "--quiet", "-m", "agent result"]);
    fs::write(managed.join("unfinished.txt"), "preserve me\n").expect("write unfinished file");
    assert_eq!(health(&trusted), WorkspaceHealth::NeedsRepair);

    let repaired = trust(&layout, &repository, trusted).expect("repair trusted workspace");

    assert_eq!(health(&repaired), WorkspaceHealth::Dirty);
    assert_eq!(
        fs::read_to_string(managed.join("file.txt")).expect("read committed result"),
        "agent result\n"
    );
    assert_eq!(
        fs::read_to_string(managed.join("unfinished.txt")).expect("read unfinished result"),
        "preserve me\n"
    );
}

#[test]
fn trust_uses_the_exact_selected_linked_worktree_head() {
    let temporary = tempfile::tempdir().expect("temporary root");
    let source = initialized_repository(temporary.path());
    fs::write(source.join("file.txt"), "selected worktree\n").expect("write selected revision");
    git(&source, &["commit", "--quiet", "-am", "selected revision"]);
    let selected_head = git_stdout(&source, &["rev-parse", "HEAD"]);
    let selected = temporary.path().join("selected");
    git(
        &source,
        &[
            "worktree",
            "add",
            "--quiet",
            "--detach",
            selected.to_str().expect("selected path"),
            &selected_head,
        ],
    );
    git(&source, &["reset", "--quiet", "--hard", "HEAD^"]);
    let primary_head = git_stdout(&source, &["rev-parse", "HEAD"]);
    assert_ne!(selected_head, primary_head);

    let repository = DiscoveredRepository::open(&selected).expect("linked worktree");
    assert_eq!(
        Path::new(repository.root_text()),
        fs::canonicalize(&selected).expect("canonical selected path")
    );
    let layout = AppLayout::for_test(&temporary.path().join("application"))
        .prepare()
        .expect("application layout");
    let trusted = trust(&layout, &repository, new_profile(&repository).expect("profile"))
        .expect("trusted workspace");
    let managed = Path::new(trusted.managed_root().expect("managed root"));

    assert_eq!(git_stdout(managed, &["rev-parse", "HEAD"]), selected_head);
    assert_eq!(
        fs::read_to_string(managed.join("file.txt")).expect("managed file"),
        "selected worktree\n"
    );
}

#[test]
fn moved_source_repository_is_reported_as_repairable() {
    let temporary = tempfile::tempdir().expect("temporary root");
    let source = initialized_repository(temporary.path());
    let repository = DiscoveredRepository::open(&source).expect("repository");
    let profile = new_profile(&repository).expect("profile");
    fs::rename(&source, temporary.path().join("moved")).expect("move source repository");
    assert_eq!(health(&profile), WorkspaceHealth::NeedsRepair);
}

fn initialized_repository(root: &Path) -> PathBuf {
    let source = root.join("source");
    fs::create_dir_all(&source).expect("source directory");
    git(&source, &["init", "--quiet"]);
    git(&source, &["config", "user.name", "Peritus Test"]);
    git(&source, &["config", "user.email", "peritus@example.invalid"]);
    fs::write(source.join("file.txt"), "initial\n").expect("source file");
    git(&source, &["add", "file.txt"]);
    git(&source, &["commit", "--quiet", "-m", "initial"]);
    fs::canonicalize(source).expect("canonical source")
}

fn git(root: &Path, arguments: &[&str]) {
    let status = Command::new("git").current_dir(root).args(arguments).status().expect("run git");
    assert!(status.success(), "git {arguments:?} failed with {status}");
}

fn git_stdout(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git").current_dir(root).args(arguments).output().expect("run git");
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("Git stdout UTF-8").trim().to_owned()
}
