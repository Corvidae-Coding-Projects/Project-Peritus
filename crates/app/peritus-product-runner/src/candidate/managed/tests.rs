use super::*;

mod flattened;
mod linked;
mod retained;

pub(super) fn repository() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("repository");
    git(root.path(), &["init", "--quiet"], None).expect("init");
    git(root.path(), &["config", "core.autocrlf", "false"], None).expect("literal fixture bytes");
    git(root.path(), &["config", "user.name", "Peritus Test"], None).expect("name");
    git(root.path(), &["config", "user.email", "test@example.invalid"], None).expect("email");
    fs::write(root.path().join("tracked.txt"), "committed\n").expect("file");
    fs::write(root.path().join("unrelated.txt"), "committed\n").expect("file");
    git(root.path(), &["add", "."], None).expect("add");
    git(root.path(), &["commit", "--quiet", "-m", "initial"], None).expect("commit");
    root
}

#[test]
fn restarted_task_excludes_prior_edits_and_discard_restores_staging_and_untracked_preimages() {
    let root = repository();
    let root = root.path();
    fs::write(root.join("tracked.txt"), "user staged\n").expect("staged");
    git(root, &["add", "tracked.txt"], None).expect("stage");
    fs::write(root.join("tracked.txt"), "user unstaged\n").expect("unstaged");
    fs::write(root.join("unrelated.txt"), "unrelated user work\n").expect("unrelated");
    fs::write(root.join("AGENTS.md"), "user guidance\n").expect("prior untracked");
    let state = tempfile::tempdir().expect("state");
    let trace = state.path().join("task.trace");
    let baseline = super::super::CandidateBaseline::capture_task(root, &trace).expect("capture");
    assert!(baseline.changed_paths(root).expect("no task edits").is_empty());
    fs::write(root.join("tracked.txt"), "task edit\n").expect("task");
    fs::remove_file(root.join("AGENTS.md")).expect("task deletion");
    fs::write(root.join("new.txt"), "new task file\n").expect("task file");
    git(root, &["add", "."], None).expect("task staging");
    let expected =
        vec![PathBuf::from("AGENTS.md"), PathBuf::from("new.txt"), PathBuf::from("tracked.txt")];
    assert_eq!(baseline.changed_paths(root).expect("task paths"), expected);
    let restored =
        ManagedBaseline::load(&trace.with_extension("baseline")).expect("load").expect("baseline");
    let patch = String::from_utf8(restored.patch(root).expect("patch")).expect("text");
    assert!(patch.contains("-user unstaged"));
    assert!(patch.contains("+task edit"));
    assert!(!patch.contains("unrelated.txt"));
    assert!(!patch.contains("-committed"));
    restored.discard(root, &expected).expect("discard");
    assert_eq!(fs::read_to_string(root.join("tracked.txt")).expect("file"), "user unstaged\n");
    assert_eq!(fs::read_to_string(root.join("AGENTS.md")).expect("file"), "user guidance\n");
    assert!(!root.join("new.txt").exists());
    assert_eq!(git(root, &["show", ":tracked.txt"], None).expect("index"), b"user staged\n");
    assert_eq!(
        git(root, &["show", ":unrelated.txt"], None).expect("unrelated index"),
        b"unrelated user work\n"
    );
    assert!(
        !Command::new("git")
            .args(["ls-files", "--error-unmatch", "AGENTS.md"])
            .current_dir(root)
            .output()
            .expect("index")
            .status
            .success()
    );
}

#[test]
fn nested_repository_changes_are_owned_relative_to_their_dirty_start() {
    let root = repository();
    let child = repository();
    let nested = root.path().join("nested");
    fs::rename(child.path(), &nested).expect("nested repository");
    fs::write(nested.join("tracked.txt"), "prior nested edit\n").expect("prior");
    let baseline = ManagedBaseline::capture(root.path(), true).expect("capture");
    assert!(baseline.changed_paths(root.path()).expect("clean task").is_empty());
    fs::write(nested.join("tracked.txt"), "task nested edit\n").expect("task");
    let paths = baseline.changed_paths(root.path()).expect("changes");
    assert_eq!(paths, vec![PathBuf::from("nested/tracked.txt")]);
    let patch = String::from_utf8(baseline.patch(root.path()).expect("patch")).expect("text");
    assert!(patch.contains("a/nested/tracked.txt"));
    assert!(patch.contains("-prior nested edit"));
    baseline.discard(root.path(), &paths).expect("discard");
    assert_eq!(
        fs::read_to_string(nested.join("tracked.txt")).expect("file"),
        "prior nested edit\n"
    );
}

#[cfg(unix)]
#[test]
fn permissions_symlinks_and_raw_bytes_survive_filters_and_discard() {
    use std::os::unix::fs::{PermissionsExt as _, symlink};
    let root = repository();
    let root = root.path();
    git(root, &["config", "filter.broken.clean", "false"], None).expect("filter");
    fs::write(root.join(".gitattributes"), "*.txt filter=broken\n").expect("attributes");
    fs::write(root.join("tracked.txt"), b"raw\r\n\x00bytes").expect("raw");
    fs::set_permissions(root.join("tracked.txt"), fs::Permissions::from_mode(0o640))
        .expect("permissions");
    symlink("tracked.txt", root.join("link")).expect("link");
    let baseline = ManagedBaseline::capture(root, true).expect("raw capture");
    fs::write(root.join("tracked.txt"), b"changed").expect("edit");
    fs::set_permissions(root.join("tracked.txt"), fs::Permissions::from_mode(0o755))
        .expect("permissions");
    fs::remove_file(root.join("link")).expect("unlink");
    symlink("unrelated.txt", root.join("link")).expect("link");
    let paths = baseline.changed_paths(root).expect("changes");
    baseline.discard(root, &paths).expect("restore");
    assert_eq!(fs::read(root.join("tracked.txt")).expect("bytes"), b"raw\r\n\x00bytes");
    assert_eq!(
        fs::metadata(root.join("tracked.txt")).expect("metadata").permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(fs::read_link(root.join("link")).expect("link"), Path::new("tracked.txt"));
}

#[test]
fn resolving_a_merge_can_be_discarded_back_to_the_original_conflict_stages() {
    let root = repository();
    let root = root.path();
    let branch =
        text(git(root, &["branch", "--show-current"], None).expect("branch")).expect("name");
    git(root, &["checkout", "-b", "other"], None).expect("branch");
    fs::write(root.join("tracked.txt"), "other branch\n").expect("other");
    git(root, &["commit", "-am", "other"], None).expect("commit");
    git(root, &["checkout", &branch], None).expect("checkout");
    fs::write(root.join("tracked.txt"), "main branch\n").expect("main");
    git(root, &["commit", "-am", "main"], None).expect("commit");
    assert!(git(root, &["merge", "other"], None).is_err());
    let conflict = fs::read(root.join("tracked.txt")).expect("conflict");
    let index = git(root, &["ls-files", "--stage", "-z"], None).expect("index");
    let baseline = ManagedBaseline::capture(root, true).expect("conflicted baseline");
    fs::write(root.join("tracked.txt"), "resolved\n").expect("resolve");
    git(root, &["add", "tracked.txt"], None).expect("stage");
    baseline.discard(root, &[PathBuf::from("tracked.txt")]).expect("discard resolution");
    assert_eq!(fs::read(root.join("tracked.txt")).expect("contents"), conflict);
    assert_eq!(git(root, &["ls-files", "--stage", "-z"], None).expect("restored stages"), index);
}

#[test]
fn replacing_a_file_with_a_directory_and_back_preserves_the_task_baseline() {
    let root = repository();
    let root = root.path();
    let baseline = ManagedBaseline::capture(root, true).expect("baseline");
    fs::remove_file(root.join("tracked.txt")).expect("remove file");
    fs::create_dir(root.join("tracked.txt")).expect("replacement directory");
    fs::write(root.join("tracked.txt/child.txt"), "task child\n").expect("child");
    let paths = baseline.changed_paths(root).expect("file became directory");
    baseline.discard(root, &paths).expect("restore file");
    assert_eq!(fs::read_to_string(root.join("tracked.txt")).expect("restored file"), "committed\n");

    fs::create_dir(root.join("folder")).expect("directory");
    fs::write(root.join("folder/child.txt"), "prior user work\n").expect("prior child");
    git(root, &["add", "folder"], None).expect("staged child");
    let baseline = ManagedBaseline::capture(root, true).expect("directory baseline");
    fs::remove_file(root.join("folder/child.txt")).expect("remove child");
    fs::remove_dir(root.join("folder")).expect("remove directory");
    fs::write(root.join("folder"), "replacement file\n").expect("replacement");
    let paths = baseline.changed_paths(root).expect("directory became file");
    baseline.discard(root, &paths).expect("restore directory");
    assert_eq!(
        fs::read_to_string(root.join("folder/child.txt")).expect("restored child"),
        "prior user work\n"
    );
}

#[test]
fn new_ignore_rules_do_not_misclassify_existing_untracked_files_as_deleted() {
    let root = repository();
    fs::write(root.path().join("user.txt"), "user draft\n").expect("prior file");
    let baseline = ManagedBaseline::capture(root.path(), true).expect("baseline");
    fs::write(root.path().join(".gitignore"), "user.txt\n").expect("task ignore");
    let paths = baseline.changed_paths(root.path()).expect("paths");
    assert_eq!(paths, vec![PathBuf::from(".gitignore")]);
    baseline.discard(root.path(), &paths).expect("discard");
    assert_eq!(
        fs::read_to_string(root.path().join("user.txt")).expect("preserved"),
        "user draft\n"
    );
}

#[test]
fn corrupt_retained_index_rejects_restore_before_any_workspace_change() {
    let root = repository();
    let baseline = ManagedBaseline::capture(root.path(), true).expect("baseline");
    fs::write(root.path().join("tracked.txt"), "task edit\n").expect("edit");
    let mut value = serde_json::to_value(&baseline).expect("value");
    value["staged"][0]["path"] = serde_json::json!("../outside");
    let corrupt: ManagedBaseline = serde_json::from_value(value).expect("structural decode");
    assert!(corrupt.discard(root.path(), &[PathBuf::from("tracked.txt")]).is_err());
    assert_eq!(
        fs::read_to_string(root.path().join("tracked.txt")).expect("unchanged"),
        "task edit\n"
    );
}

#[test]
fn missing_preimage_does_not_delete_task_files_or_restore_other_paths() {
    let root = repository();
    let root = root.path();
    let mut baseline = ManagedBaseline::capture(root, true).expect("baseline");
    fs::write(root.join("tracked.txt"), "task edit\n").expect("edit");
    fs::write(root.join("new.txt"), "task file\n").expect("new");
    baseline.entries.get_mut("tracked.txt").expect("entry").object =
        "a".repeat(baseline.tree.len());
    let before_index = git(root, &["ls-files", "--stage", "-z"], None).expect("index");
    assert!(
        baseline.discard(root, &[PathBuf::from("new.txt"), PathBuf::from("tracked.txt")]).is_err()
    );
    assert_eq!(fs::read(root.join("tracked.txt")).expect("unchanged"), b"task edit\n");
    assert_eq!(fs::read(root.join("new.txt")).expect("retained"), b"task file\n");
    assert_eq!(git(root, &["ls-files", "--stage", "-z"], None).expect("index"), before_index);
}

#[test]
fn nested_head_change_is_discarded_without_rewriting_the_task_branch() {
    let root = repository();
    let child = repository();
    let nested = root.path().join("nested");
    fs::rename(child.path(), &nested).expect("nested");
    let baseline = ManagedBaseline::capture(root.path(), true).expect("baseline");
    fs::write(nested.join("tracked.txt"), "new commit\n").expect("edit");
    git(&nested, &["commit", "-am", "task commit"], None).expect("commit");
    fs::write(root.path().join("new.txt"), "retain until entire restore is validated\n")
        .expect("new");
    let paths = baseline.changed_paths(root.path()).expect("changes");
    let branch = text(git(&nested, &["symbolic-ref", "HEAD"], None).unwrap()).unwrap();
    let task_commit = git(&nested, &["rev-parse", "HEAD"], None).unwrap();
    let recovery = baseline.discard(root.path(), &paths).expect("restore including HEAD");
    assert_eq!(recovery.len(), 1);
    assert!(recovery[0].join("head.json").is_file());
    assert!(!root.path().join("new.txt").exists());
    assert_eq!(fs::read(nested.join("tracked.txt")).unwrap(), b"committed\n");
    assert_eq!(git(&nested, &["rev-parse", &branch], None).unwrap(), task_commit);
    assert!(baseline.changed_paths(root.path()).unwrap().is_empty());
}

#[test]
fn newly_created_nested_repository_exports_complete_source_files() {
    let root = repository();
    let baseline = ManagedBaseline::capture(root.path(), true).expect("baseline");
    let child = repository();
    let nested = root.path().join("generated");
    fs::rename(child.path(), &nested).expect("nested repository");
    fs::write(nested.join("tracked.txt"), b"new application source\n").expect("source");
    fs::write(nested.join("extra.txt"), b"uncommitted source\n").expect("untracked");
    let grandchild = repository();
    fs::rename(grandchild.path(), nested.join("component")).expect("deep repository");
    let binary = b"\0\xff\x01binary source\n";
    fs::write(nested.join("component/asset.bin"), binary).expect("binary asset");
    let paths = baseline.changed_paths(root.path()).expect("changed paths");
    assert!(paths.contains(&PathBuf::from("generated/component/asset.bin")));
    let patch = baseline.patch(root.path()).expect("export");
    let destination = repository();
    git(destination.path(), &["apply", "--binary", "-"], Some(&patch))
        .expect("apply exported patch");
    assert_eq!(
        fs::read(destination.path().join("generated/tracked.txt")).unwrap(),
        b"new application source\n"
    );
    assert_eq!(
        fs::read(destination.path().join("generated/extra.txt")).unwrap(),
        b"uncommitted source\n"
    );
    assert!(!destination.path().join("generated/.git").exists());
    assert_eq!(fs::read(destination.path().join("generated/component/asset.bin")).unwrap(), binary);
    assert!(!destination.path().join("generated/component/.git").exists());
}

#[test]
fn nested_repository_without_a_first_commit_exports_and_restores_source_changes() {
    let root = repository();
    let baseline = ManagedBaseline::capture(root.path(), true).expect("baseline");
    let nested = root.path().join("generated");
    fs::create_dir(&nested).unwrap();
    git(&nested, &["init", "--quiet"], None).expect("init without commit");
    fs::write(nested.join("source.txt"), b"initial source\n").unwrap();
    let patch = baseline.patch(root.path()).expect("export uncommitted repository");
    let destination = repository();
    git(destination.path(), &["apply", "-"], Some(&patch)).expect("apply");
    assert_eq!(
        fs::read(destination.path().join("generated/source.txt")).unwrap(),
        b"initial source\n"
    );
    let paths = baseline.changed_paths(root.path()).expect("new repository changes");
    assert!(paths.contains(&PathBuf::from("generated/source.txt")));

    let existing = ManagedBaseline::capture(root.path(), true).expect("retain unborn repository");
    fs::write(nested.join("source.txt"), b"revised source\n").unwrap();
    let paths = existing.changed_paths(root.path()).expect("source changes");
    assert_eq!(paths, vec![PathBuf::from("generated/source.txt")]);
    let patch = existing.patch(root.path()).expect("export revision");
    git(destination.path(), &["apply", "-"], Some(&patch)).expect("apply revision");
    assert_eq!(
        fs::read(destination.path().join("generated/source.txt")).unwrap(),
        b"revised source\n"
    );
    existing.discard(root.path(), &paths).expect("restore source");
    assert_eq!(fs::read(nested.join("source.txt")).unwrap(), b"initial source\n");
}

mod prepared;
mod recovery;

#[test]
fn first_nested_commit_exports_source_without_overwriting_git_history() {
    let root = repository();
    let nested = root.path().join("generated");
    fs::create_dir(&nested).unwrap();
    git(&nested, &["init", "--quiet"], None).unwrap();
    fs::write(nested.join("source.txt"), b"initial\n").unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).expect("unborn baseline");
    fs::write(nested.join("source.txt"), b"committed change\n").unwrap();
    git(&nested, &["add", "."], None).unwrap();
    git(
        &nested,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "first",
        ],
        None,
    )
    .unwrap();
    let patch = baseline.patch(root.path()).expect("first commit export");
    let destination = repository();
    fs::create_dir(destination.path().join("generated")).unwrap();
    fs::write(destination.path().join("generated/source.txt"), b"initial\n").unwrap();
    git(destination.path(), &["apply", "-"], Some(&patch)).expect("apply source patch");
    assert_eq!(
        fs::read(destination.path().join("generated/source.txt")).unwrap(),
        b"committed change\n"
    );
    let paths = baseline.changed_paths(root.path()).unwrap();
    let task_commit = git(&nested, &["rev-parse", "HEAD"], None).unwrap();
    let branch = text(git(&nested, &["symbolic-ref", "HEAD"], None).unwrap()).unwrap();
    let recovered = baseline.discard(root.path(), &paths).expect("restore unborn state");
    assert_eq!(recovered.len(), 1);
    assert!(capture::nested_head(&nested).unwrap().is_none());
    assert_eq!(git(&nested, &["rev-parse", &branch], None).unwrap(), task_commit);
    assert_eq!(fs::read(nested.join("source.txt")).unwrap(), b"initial\n");
    assert!(git(&nested, &["ls-files"], None).unwrap().is_empty());
    assert!(baseline.changed_paths(root.path()).unwrap().is_empty());
}

#[test]
fn missing_nested_commit_object_is_not_mistaken_for_an_unborn_repository() {
    let root = repository();
    let nested = root.path().join("generated");
    fs::create_dir(&nested).unwrap();
    git(&nested, &["init", "--quiet"], None).unwrap();
    let branch = text(git(&nested, &["symbolic-ref", "HEAD"], None).unwrap()).unwrap();
    let reference = nested.join(".git").join(branch);
    fs::create_dir_all(reference.parent().unwrap()).unwrap();
    fs::write(reference, format!("{}\n", "1".repeat(40))).unwrap();
    fs::write(nested.join("source.txt"), b"retain source\n").unwrap();
    assert!(ManagedBaseline::capture(root.path(), true).is_err());
    assert_eq!(fs::read(nested.join("source.txt")).unwrap(), b"retain source\n");
}
