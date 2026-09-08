use super::*;

fn scope(root: &Path, state: &Path) -> ScopedBaseline {
    ScopedBaseline::new(root.to_owned(), state.join("files.jsonl"), vec![root.join("private")], 1)
}

#[test]
fn enrollment_is_not_progress_but_creation_edit_deletion_and_reversion_are() {
    let root = tempfile::tempdir().expect("root");
    let state = tempfile::tempdir().expect("state");
    let scoped = scope(root.path(), state.path());
    let before = scoped.progress_checkpoint(root.path()).expect("empty checkpoint");
    scoped.enroll("Cargo.toml").expect("missing read enrollment");
    assert_eq!(scoped.progress_checkpoint(root.path()).unwrap(), before);
    fs::write(root.path().join("existing.txt"), "before").unwrap();
    scoped.enroll("existing.txt").unwrap();
    assert_eq!(scoped.progress_checkpoint(root.path()).unwrap(), before);
    fs::write(root.path().join("Cargo.toml"), "created").unwrap();
    let created = scoped.progress_checkpoint(root.path()).unwrap();
    assert_ne!(created, before);
    fs::write(root.path().join("Cargo.toml"), "edited").unwrap();
    assert_ne!(scoped.progress_checkpoint(root.path()).unwrap(), created);
    fs::remove_file(root.path().join("Cargo.toml")).unwrap();
    assert_eq!(scoped.progress_checkpoint(root.path()).unwrap(), before);
    fs::remove_file(root.path().join("existing.txt")).unwrap();
    assert_ne!(scoped.progress_checkpoint(root.path()).unwrap(), before);
}

#[test]
fn exact_file_baseline_survives_reopen_without_inventory_or_rebaselining() {
    let root = tempfile::tempdir().expect("root");
    let state = tempfile::tempdir().expect("state");
    fs::write(root.path().join("note.txt"), "before").expect("before");
    fs::write(root.path().join("unrelated.txt"), "private-unrelated").expect("unrelated");
    let original = scope(root.path(), state.path());
    original.enroll("note.txt").expect("enroll before effects");
    fs::write(root.path().join("note.txt"), "after").expect("effect");
    fs::write(root.path().join("unrelated.txt"), "external change").expect("external effect");
    let reopened: ScopedBaseline =
        serde_json::from_str(&serde_json::to_string(&original).expect("encode")).expect("reopen");
    reopened.enroll("note.txt").expect("idempotent enrollment");
    assert_eq!(reopened.paths().expect("paths"), [PathBuf::from("note.txt")]);
    assert_eq!(reopened.changed_paths(root.path()).expect("changed"), [PathBuf::from("note.txt")]);
    let diff = reopened.diff(root.path()).expect("comparison");
    assert!(diff.contains("-before"));
    assert!(diff.contains("+after"));
    assert!(!diff.contains("unrelated"));
    assert!(!root.path().join(".git").exists());
    assert!(
        !fs::read_to_string(state.path().join("files.jsonl"))
            .expect("journal")
            .contains("private-unrelated")
    );
}

#[test]
fn scope_rejects_protected_traversal_torn_and_forged_records() {
    let root = tempfile::tempdir().expect("root");
    let state = tempfile::tempdir().expect("state");
    let scoped = scope(root.path(), state.path());
    for path in ["", ".", "../outside", "private/secret"] {
        assert!(scoped.enroll(path).is_err(), "must reject {path}");
    }
    scoped.enroll("note.txt").expect("valid absent path");
    let journal = state.path().join("files.jsonl");
    let valid = fs::read(&journal).expect("valid record");
    fs::write(&journal, &valid[..valid.len() - 1]).expect("torn write");
    assert!(scoped.paths().is_err());
    let mut forged: serde_json::Value = serde_json::from_slice(&valid).expect("record");
    forged["path"] = serde_json::Value::String("../outside".to_owned());
    fs::write(&journal, format!("{forged}\n")).expect("forged path");
    assert!(scoped.paths().is_err(), "restored paths must be validated before hashing");
}

#[cfg(unix)]
#[test]
fn a_scoped_file_replaced_by_a_symlink_fails_before_reading_the_destination() {
    let root = tempfile::tempdir().expect("root");
    let state = tempfile::tempdir().expect("state");
    let scoped = scope(root.path(), state.path());
    scoped.enroll("note.txt").expect("absent path");
    fs::write(state.path().join("secret"), "outside secret").expect("secret");
    std::os::unix::fs::symlink(state.path().join("secret"), root.path().join("note.txt"))
        .expect("symlink");
    assert!(scoped.paths().is_err());
    assert!(scoped.diff(root.path()).is_err());
}

#[test]
fn full_file_changes_beyond_the_preview_invalidate_freshness() {
    let root = tempfile::tempdir().expect("root");
    let state = tempfile::tempdir().expect("state");
    let scoped = scope(root.path(), state.path());
    let mut bytes = vec![b'a'; MAX_PREVIEW_BYTES + 1];
    fs::write(root.path().join("large.txt"), &bytes).expect("before");
    scoped.enroll("large.txt").expect("enroll");
    *bytes.last_mut().expect("last") = b'b';
    fs::write(root.path().join("large.txt"), bytes).expect("after");
    assert_eq!(scoped.changed_paths(root.path()).expect("changed").len(), 1);
    assert!(scoped.diff(root.path()).expect("diff").contains("bounded preview"));
}

#[test]
fn a_directory_declaration_cannot_conceal_undeclared_descendant_effects() {
    let root = tempfile::tempdir().expect("root");
    let state = tempfile::tempdir().expect("state");
    let scoped = scope(root.path(), state.path());
    fs::create_dir(root.path().join("output")).expect("empty directory");
    scoped.enroll("output").expect("exact empty directory");
    fs::write(root.path().join("output/new.txt"), "undeclared output").expect("descendant effect");
    assert!(scoped.changed_paths(root.path()).is_err());
    assert!(scoped.diff(root.path()).is_err());
}
