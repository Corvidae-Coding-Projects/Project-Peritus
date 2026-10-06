use peritus_types::{Generation, RevisionNumber, WorkspaceId};

use crate::{FileMode, FinalFile, LineEndingPolicy, PatchOperation, WorkspacePath};

use super::PatchSet;

fn workspace() -> WorkspaceId {
    WorkspaceId::new([7; 16]).expect("nonzero")
}

#[test]
fn canonical_identity_does_not_depend_on_input_order() {
    let file = |byte| {
        FinalFile::new(vec![byte], FileMode::Regular, LineEndingPolicy::Preserve).expect("file")
    };
    let a = PatchOperation::create(WorkspacePath::new("a").expect("path"), file(1));
    let b = PatchOperation::create(WorkspacePath::new("b").expect("path"), file(2));
    let make = |operations| {
        PatchSet::new(workspace(), Generation::first(), RevisionNumber::first(), operations)
            .expect("patch")
    };
    assert_eq!(make(vec![a.clone(), b.clone()]).identity(), make(vec![b, a]).identity());
}

#[test]
fn directory_permissions_are_bound_into_inline_and_snapshot_authority() {
    for constructor in [PatchSet::new, PatchSet::from_snapshot] {
        let patch = |bits| {
            constructor(
                workspace(),
                Generation::first(),
                RevisionNumber::first(),
                vec![PatchOperation::create_directory(
                    WorkspacePath::new("empty").expect("path"),
                    crate::DirectoryMode::new(bits).expect("mode"),
                )],
            )
            .expect("directory patch")
        };
        assert_ne!(patch(0o750).identity(), patch(0o700).identity());
    }
    assert!(crate::DirectoryMode::new(0o10000).is_err());
}

#[test]
fn paged_authority_binds_the_complete_intent_and_is_order_independent() {
    let file = FinalFile::new(Vec::new(), FileMode::Regular, LineEndingPolicy::Preserve).unwrap();
    let operations: Vec<_> = (0..1_025)
        .map(|index| {
            PatchOperation::create(
                WorkspacePath::new(format!("file-{index:04}")).unwrap(),
                file.clone(),
            )
        })
        .collect();
    let make = |id, generation, revision, operations| {
        PatchSet::new(id, generation, revision, operations).unwrap()
    };
    let original =
        make(workspace(), Generation::first(), RevisionNumber::first(), operations.clone());
    assert!(original.is_paged());
    let mut reordered = operations.clone();
    reordered.reverse();
    assert_eq!(
        original.identity(),
        make(workspace(), Generation::first(), RevisionNumber::first(), reordered).identity()
    );
    for altered in [
        make(
            WorkspaceId::new([8; 16]).unwrap(),
            Generation::first(),
            RevisionNumber::first(),
            operations.clone(),
        ),
        make(workspace(), Generation::new(2).unwrap(), RevisionNumber::first(), operations.clone()),
        make(workspace(), Generation::first(), RevisionNumber::new(2).unwrap(), operations.clone()),
    ] {
        assert_ne!(original.identity(), altered.identity());
    }
    for operation in [
        PatchOperation::create(WorkspacePath::new("different-path").unwrap(), file.clone()),
        PatchOperation::create(
            WorkspacePath::new("file-0000").unwrap(),
            FinalFile::new(Vec::new(), FileMode::Regular, LineEndingPolicy::Lf).unwrap(),
        ),
        PatchOperation::create(
            WorkspacePath::new("file-0000").unwrap(),
            FinalFile::new(vec![1], FileMode::Regular, LineEndingPolicy::Preserve).unwrap(),
        ),
        PatchOperation::create(
            WorkspacePath::new("file-0000").unwrap(),
            FinalFile::new(Vec::new(), FileMode::Executable, LineEndingPolicy::Preserve).unwrap(),
        ),
        PatchOperation::replace(
            WorkspacePath::new("file-0000").unwrap(),
            crate::Preimage::from_bytes(b"before", FileMode::Regular),
            file,
        )
        .unwrap(),
    ] {
        let mut altered = operations.clone();
        altered[0] = operation;
        assert_ne!(
            original.identity(),
            make(workspace(), Generation::first(), RevisionNumber::first(), altered).identity()
        );
    }
}
