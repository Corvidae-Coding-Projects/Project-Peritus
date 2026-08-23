//! Real temporary-filesystem transaction integration tests.

use peritus_patch::{
    ErrorCode, FileMode, FinalFile, LineEndingPolicy, PatchOperation, PatchSet, Preimage,
    WorkspacePath, apply_patch,
};
use peritus_types::{Generation, RevisionNumber, WorkspaceId};

fn final_file(bytes: &[u8], mode: FileMode) -> FinalFile {
    FinalFile::new(bytes.to_vec(), mode, LineEndingPolicy::Preserve).expect("bounded final file")
}

fn workspace_id() -> WorkspaceId {
    WorkspaceId::new([9; 16]).expect("workspace id")
}

fn plan(operations: Vec<PatchOperation>) -> peritus_patch::PatchPlan {
    PatchSet::new(workspace_id(), Generation::first(), RevisionNumber::first(), operations)
        .expect("patch")
        .plan(workspace_id(), Generation::first(), RevisionNumber::first())
        .expect("plan")
}

#[test]
fn applies_create_replace_and_delete_as_one_real_transaction() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    std::fs::write(workspace.path().join("replace"), b"before").expect("replace preimage");
    std::fs::write(workspace.path().join("delete"), b"remove").expect("delete preimage");
    let operations = vec![
        PatchOperation::create(
            WorkspacePath::new("nested/create").expect("path"),
            final_file(b"created", FileMode::Executable),
        ),
        PatchOperation::replace(
            WorkspacePath::new("replace").expect("path"),
            Preimage::from_bytes(b"before", FileMode::Regular),
            final_file(b"after", FileMode::Regular),
        )
        .expect("replace"),
        PatchOperation::delete(
            WorkspacePath::new("delete").expect("path"),
            Preimage::from_bytes(b"remove", FileMode::Regular),
        )
        .expect("delete"),
    ];
    let applied = apply_patch(workspace.path(), transactions.path(), &plan(operations))
        .expect("atomic application");
    assert_eq!(std::fs::read(workspace.path().join("nested/create")).expect("create"), b"created");
    assert_eq!(std::fs::read(workspace.path().join("replace")).expect("replace"), b"after");
    assert!(!workspace.path().join("delete").exists());
    assert!(!applied.cleanup_pending());
    assert_eq!(peritus_codec::sha256(applied.installed_manifest()), applied.manifest_digest());
    assert_eq!(std::fs::read_dir(transactions.path()).expect("transactions").count(), 0);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(workspace.path().join("nested/create"))
            .expect("metadata")
            .permissions()
            .mode();
        assert_ne!(mode & 0o111, 0);
    }
}

#[test]
fn rejects_one_bad_preimage_before_any_target_changes() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    std::fs::write(workspace.path().join("old"), b"actual").expect("preimage");
    let operations = vec![
        PatchOperation::create(
            WorkspacePath::new("new").expect("path"),
            final_file(b"new", FileMode::Regular),
        ),
        PatchOperation::replace(
            WorkspacePath::new("old").expect("path"),
            Preimage::from_bytes(b"wrong", FileMode::Regular),
            final_file(b"after", FileMode::Regular),
        )
        .expect("replace"),
    ];
    let error = apply_patch(workspace.path(), transactions.path(), &plan(operations))
        .expect_err("mismatched preimage");
    assert_eq!(error.code(), ErrorCode::PreimageMismatch);
    assert_eq!(std::fs::read(workspace.path().join("old")).expect("old"), b"actual");
    assert!(!workspace.path().join("new").exists());
    assert_eq!(std::fs::read_dir(transactions.path()).expect("transactions").count(), 0);
}

#[cfg(unix)]
#[test]
fn rejects_symlink_targets_without_following_them() {
    use std::os::unix::fs::symlink;

    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    let outside = tempfile::NamedTempFile::new().expect("outside");
    std::fs::write(outside.path(), b"outside").expect("outside bytes");
    symlink(outside.path(), workspace.path().join("link")).expect("symlink");
    let operation = PatchOperation::replace(
        WorkspacePath::new("link").expect("path"),
        Preimage::from_bytes(b"outside", FileMode::Regular),
        final_file(b"changed", FileMode::Regular),
    )
    .expect("replace");
    let error = apply_patch(workspace.path(), transactions.path(), &plan(vec![operation]))
        .expect_err("symlink rejected");
    assert_eq!(error.code(), ErrorCode::UnsafeFilesystemTarget);
    assert_eq!(std::fs::read(outside.path()).expect("outside"), b"outside");
}

#[test]
fn rejects_targets_inside_a_nested_git_worktree() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    std::fs::create_dir(workspace.path().join("nested")).expect("nested");
    std::fs::write(workspace.path().join("nested/.git"), b"gitdir: elsewhere")
        .expect("worktree metadata");
    let operation = PatchOperation::create(
        WorkspacePath::new("nested/file").expect("path"),
        final_file(b"forbidden", FileMode::Regular),
    );
    let error = apply_patch(workspace.path(), transactions.path(), &plan(vec![operation]))
        .expect_err("nested repository rejected");
    assert_eq!(error.code(), ErrorCode::UnsafeFilesystemTarget);
    assert!(!workspace.path().join("nested/file").exists());
}

#[test]
fn rejects_oversized_preimage_before_application() {
    let workspace = tempfile::tempdir().expect("workspace");
    let bytes = vec![5; peritus_patch::MAX_PATCH_BYTES + 1];
    std::fs::write(workspace.path().join("large"), &bytes).expect("large preimage");
    let operation = PatchOperation::replace(
        WorkspacePath::new("large").expect("path"),
        Preimage::present(peritus_codec::sha256(&[]), bytes.len() as u64, FileMode::Regular),
        final_file(b"replacement", FileMode::Regular),
    )
    .expect("replace shape");
    let error = PatchSet::new(
        workspace_id(),
        Generation::first(),
        RevisionNumber::first(),
        vec![operation],
    )
    .expect_err("oversized preimage rejected");
    assert_eq!(error.code(), ErrorCode::InvalidPatchBounds);
    assert_eq!(std::fs::read(workspace.path().join("large")).expect("unchanged"), bytes);
}
