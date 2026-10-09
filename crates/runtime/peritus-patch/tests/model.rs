//! Public typed-model and planner integration tests.

use peritus_patch::{
    ErrorCode, FileMode, FinalFile, LineEndingPolicy, PatchOperation, PatchSet, WorkspacePath,
};
use peritus_types::{Generation, RevisionNumber, WorkspaceId};

#[test]
fn line_endings_are_explicit_and_part_of_final_identity() {
    let lf =
        FinalFile::new(b"one\r\ntwo\rthree\n".to_vec(), FileMode::Regular, LineEndingPolicy::Lf)
            .expect("LF text");
    let crlf =
        FinalFile::new(b"one\r\ntwo\rthree\n".to_vec(), FileMode::Regular, LineEndingPolicy::Crlf)
            .expect("CRLF text");
    assert_eq!(lf.bytes(), b"one\ntwo\nthree\n");
    assert_eq!(crlf.bytes(), b"one\r\ntwo\r\nthree\r\n");
    assert_ne!(lf.digest(), crlf.digest());
}

#[test]
fn stale_workspace_tuple_never_produces_a_plan() {
    let workspace = WorkspaceId::new([1; 16]).expect("workspace");
    let operation = PatchOperation::create(
        WorkspacePath::new("file").expect("path"),
        FinalFile::new(vec![], FileMode::Regular, LineEndingPolicy::Preserve).expect("file"),
    );
    let patch =
        PatchSet::new(workspace, Generation::first(), RevisionNumber::first(), vec![operation])
            .expect("patch");
    let stale = patch
        .plan(workspace, Generation::new(2).expect("generation"), RevisionNumber::first())
        .expect_err("stale");
    assert_eq!(stale.code(), ErrorCode::StaleWorkspace);
}

#[test]
fn duplicate_and_ancestor_targets_are_rejected() {
    let workspace = WorkspaceId::new([2; 16]).expect("workspace");
    let operation = |path| {
        PatchOperation::create(
            WorkspacePath::new(path).expect("path"),
            FinalFile::new(vec![], FileMode::Regular, LineEndingPolicy::Preserve).expect("file"),
        )
    };
    let duplicate = PatchSet::new(
        workspace,
        Generation::first(),
        RevisionNumber::first(),
        vec![operation("same"), operation("same")],
    )
    .expect_err("duplicate");
    assert_eq!(duplicate.code(), ErrorCode::DuplicateTarget);
    let ancestor = PatchSet::new(
        workspace,
        Generation::first(),
        RevisionNumber::first(),
        vec![operation("directory"), operation("directory/file")],
    )
    .expect_err("ancestor");
    assert_eq!(ancestor.code(), ErrorCode::TargetShapeConflict);
}

#[test]
fn rejects_recovery_manifest_directory_overflow_during_planning() {
    let workspace = WorkspaceId::new([3; 16]).expect("workspace");
    let operations = (0..peritus_patch::MAX_PATCH_OPERATIONS)
        .map(|index| {
            let path = format!("p{index}/{}/file", vec!["d"; 63].join("/"));
            PatchOperation::create(
                WorkspacePath::new(path).expect("bounded path"),
                FinalFile::new(Vec::new(), FileMode::Regular, LineEndingPolicy::Preserve)
                    .expect("file"),
            )
        })
        .collect();
    let error = PatchSet::new(workspace, Generation::first(), RevisionNumber::first(), operations)
        .expect_err("directory collection exceeds recovery manifest limit");
    assert_eq!(error.code(), ErrorCode::InvalidPatchBounds);
}
