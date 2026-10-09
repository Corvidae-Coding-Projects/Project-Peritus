//! Real inheritance, rename, replacement, and transaction cleanup regressions.

use super::acl_fixture as fixture;

#[path = "acl_inheritance_probe.rs"]
mod inheritance_probe;

use peritus_sandbox::{
    FileOperation, FileOperationSet, FilesystemRule, PathScope, RuleEffect, SandboxPath,
};
use peritus_sandbox_windows::{PathPolicy, WindowsPath, compile_acl_plan};
use std::path::PathBuf;

struct Tree {
    _root: tempfile::TempDir,
    base: PathBuf,
    workspace: PathBuf,
    backup: PathBuf,
}

impl Tree {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let base = WindowsPath::from_canonicalized(&std::fs::canonicalize(root.path()).unwrap())
            .unwrap()
            .to_path_buf();
        let workspace = base.join("workspace");
        std::fs::create_dir_all(workspace.join("bin")).unwrap();
        std::fs::write(workspace.join("bin/tool"), b"fixture").unwrap();
        std::fs::create_dir_all(workspace.join("workspace/inherited/nested")).unwrap();
        std::fs::create_dir(workspace.join("workspace/protected")).unwrap();
        std::fs::write(workspace.join("workspace/inherited/existing"), b"existing").unwrap();
        std::fs::write(workspace.join("workspace/protected/custom"), b"protected").unwrap();
        fixture::set(&workspace.join("workspace"), "D:(A;OICI;FA;;;WD)");
        fixture::set(&workspace.join("workspace/inherited"), "D:ARAI(A;OICIID;FA;;;WD)");
        fixture::set(&workspace.join("workspace/protected"), "D:P(A;OICI;FA;;;WD)");
        let backup = base.join("backup");
        Self { _root: root, base, workspace, backup }
    }

    fn plan(&self) -> peritus_sandbox_windows::AclPlan {
        let nested = FilesystemRule::new(
            RuleEffect::Allow,
            SandboxPath::new("/workspace/inherited/nested").unwrap(),
            PathScope::Descendants,
            FileOperationSet::from_operations([FileOperation::Read]),
        )
        .unwrap();
        let protected =
            WindowsPath::from_os_str(self.workspace.join("workspace/private").as_os_str()).unwrap();
        let root = WindowsPath::from_os_str(self.workspace.as_os_str()).unwrap();
        let policy = PathPolicy::new(root, vec![protected]).unwrap();
        compile_acl_plan(&crate::support::checked_plan(vec![nested]), &policy, "S-1-5-32-545")
            .unwrap()
    }

    fn originals(&self) -> Vec<PathBuf> {
        [
            "bin/tool",
            "workspace",
            "workspace/inherited",
            "workspace/inherited/nested",
            "workspace/inherited/existing",
            "workspace/protected",
            "workspace/protected/custom",
        ]
        .map(|path| self.workspace.join(path))
        .to_vec()
    }
}

#[test]
fn native_acl_restores_legacy_inherited_protected_and_nested_objects_exactly() {
    let _serial = fixture::serial();
    let tree = Tree::new();
    let originals = tree.originals();
    let before = originals.iter().map(|path| fixture::snapshot(path)).collect::<Vec<_>>();
    assert_eq!(fixture::control(&before[1]) & 0x1400, 0, "legacy original must lack AI/P");
    assert_ne!(fixture::control(&before[2]) & 0x0400, 0, "child must start inherited");
    assert_ne!(fixture::control(&before[5]) & 0x1000, 0, "custom child must be protected");
    let parent_before = fixture::snapshot(&tree.workspace);
    let parent_sddl_before = fixture::sddl(&tree.workspace);
    let mut transaction = tree.plan().install(&tree.backup).unwrap();
    let new_child = tree.workspace.join("workspace/new-during-session");
    std::fs::write(&new_child, b"new").unwrap();
    assert!(
        fixture::sddl(&new_child).contains(";;;BU)"),
        "new child must inherit the temporary principal"
    );
    let outcome = transaction.restore();
    assert!(
        outcome.is_ok(),
        "ordinary child cleanup {outcome:?}; outer_parent_before={parent_sddl_before}; outer_parent_after={}; parent_after={}; child_after={}; {}",
        fixture::sddl(&tree.workspace),
        fixture::sddl(&tree.workspace.join("workspace")),
        fixture::sddl(&new_child),
        inheritance_probe::inspect(
            &tree.workspace.join("workspace"),
            &new_child,
            false,
            "S-1-5-32-545"
        )
    );
    assert!(transaction.restored());
    for (path, expected) in originals.iter().zip(&before) {
        assert_eq!(
            &fixture::snapshot(path),
            expected,
            "exact DACL/control changed: {}",
            path.display()
        );
    }
    assert_eq!(
        fixture::snapshot(&tree.workspace),
        parent_before,
        "outside parent provenance changed"
    );
    assert!(
        !fixture::sddl(&new_child).contains(";;;BU)"),
        "new child retained the temporary principal"
    );
    assert!(!tree.workspace.join("workspace/private").exists());
    assert_eq!(std::fs::read_dir(&tree.backup).unwrap().count(), 0);
    transaction.restore().unwrap();
}

#[test]
fn native_acl_restores_moved_originals_without_overwriting_path_replacements() {
    let _serial = fixture::serial();
    let tree = Tree::new();
    let original = tree.workspace.join("workspace/inherited/existing");
    let before = fixture::snapshot(&original);
    let mut transaction = tree.plan().install(&tree.backup).unwrap();
    let moved = tree.base.join("moved-original");
    std::fs::rename(&original, &moved).unwrap();
    std::fs::write(&original, b"replacement").unwrap();
    fixture::set(&original, "D:P(A;;FR;;;WD)(A;;FA;;;SY)(A;;FA;;;BA)");
    let replacement = fixture::snapshot(&original);
    transaction.restore().unwrap();
    assert_eq!(fixture::snapshot(&moved), before, "moved original lost exact backup ownership");
    assert_eq!(
        fixture::snapshot(&original),
        replacement,
        "replacement received the original's DACL"
    );
    assert_eq!(std::fs::read(&original).unwrap(), b"replacement");
    assert!(transaction.restored());
}

#[test]
fn native_acl_failed_preflight_never_normalizes_uncaptured_descendants() {
    let _serial = fixture::serial();
    let tree = Tree::new();
    let originals = tree.originals();
    let before = originals.iter().map(|path| fixture::snapshot(path)).collect::<Vec<_>>();
    // A backup below an affected directory is rejected while the pristine inventory is still
    // incomplete. Rollback must only release captures and owned anchors, never replay ACLs.
    let error = tree.plan().install(&tree.workspace.join("workspace/invalid-backup")).unwrap_err();
    assert_eq!(error.operation(), peritus_sandbox_windows::WindowsOperation::InstallAcl);
    assert_eq!(error.recovery(), peritus_sandbox_windows::WindowsRecovery::CorrectRequest);
    assert!(!error.preparation_cleanup().acl_restore());
    for (path, expected) in originals.iter().zip(&before) {
        assert_eq!(&fixture::snapshot(path), expected, "preflight changed {}", path.display());
    }
    assert!(!tree.workspace.join("workspace/private").exists());
}

#[test]
fn native_acl_owned_anchor_replacement_is_denied_until_exact_cleanup() {
    let _serial = fixture::serial();
    let tree = Tree::new();
    let mut transaction = tree.plan().install(&tree.backup).unwrap();
    let anchor = tree.workspace.join("workspace/private");
    fixture::set(&anchor, "D:P(A;OICI;FA;;;WD)");
    let moved = tree.base.join("attempted-anchor-replacement");
    assert!(std::fs::rename(&anchor, &moved).is_err(), "owned anchor identity became replaceable");
    assert!(std::fs::create_dir(&anchor).is_err());
    transaction.restore().unwrap();
    assert!(!anchor.exists());
    assert!(!moved.exists());
    assert!(transaction.restored());
}

#[test]
fn native_acl_protected_new_children_retain_conflict_without_losing_custom_aces() {
    use peritus_sandbox_windows::CleanupState;
    let _serial = fixture::serial();
    let tree = Tree::new();
    let parent = tree.workspace.join("workspace");
    fixture::set(&parent, "D:(A;OICI;FA;;;WD)(A;OICI;FR;;;BU)");
    let mut transaction = tree.plan().install(&tree.backup).unwrap();
    let file = parent.join("new-protected-file");
    let directory = parent.join("new-protected-directory");
    std::fs::write(&file, b"new").unwrap();
    std::fs::create_dir(&directory).unwrap();
    fixture::set(&file, "D:P(A;;0x10087;;;BU)(A;;FA;;;WD)(A;;FR;;;AU)");
    fixture::set(&directory, "D:P(A;OICI;0x10087;;;BU)(A;OICI;FA;;;WD)(A;OICI;FR;;;AU)");
    let before = [fixture::snapshot(&file), fixture::snapshot(&directory)];
    assert!(transaction.restore().is_err(), "protected temporary authority was silently accepted");
    assert_eq!(transaction.cleanup_state(), CleanupState::RetryRequired);
    assert!(!transaction.restored());
    assert_eq!(fixture::snapshot(&file), before[0], "custom file ACL changed during conflict");
    assert_eq!(
        fixture::snapshot(&directory),
        before[1],
        "custom directory ACL changed during conflict"
    );
    assert_eq!(
        std::fs::read_dir(&tree.backup).unwrap().count(),
        1,
        "conflict released backup owner"
    );
    // A protected child can preserve an inherited-marked temporary ACE. The detector must
    // inspect it even though GetExplicitEntriesFromAcl omits inherited entries entirely.
    fixture::set(&file, "D:P(A;;FA;;;WD)(A;;FR;;;AU)(A;ID;0x10087;;;BU)");
    fixture::set(&directory, "D:P(A;OICI;FR;;;BU)(A;OICI;FA;;;WD)(A;OICI;FR;;;AU)");
    let marked = [fixture::snapshot(&file), fixture::snapshot(&directory)];
    assert!(fixture::sddl(&file).contains("A;ID;"));
    assert!(transaction.restore().is_err(), "inherited-marked temporary authority was accepted");
    assert_eq!(transaction.cleanup_state(), CleanupState::RetryRequired);
    assert_eq!(fixture::snapshot(&file), marked[0], "inherited-marked file ACL changed");
    assert_eq!(fixture::snapshot(&directory), marked[1], "inherited-marked directory ACL changed");
    assert_eq!(std::fs::read_dir(&tree.backup).unwrap().count(), 1);
    // Keep the file legitimate while independently exercising inherited directory rejection.
    fixture::set(&file, "D:P(A;;FR;;;BU)(A;;FA;;;WD)(A;;FR;;;AU)");
    let file_resolved = fixture::snapshot(&file);
    fixture::set(&directory, "D:P(A;OICI;FA;;;WD)(A;OICI;FR;;;AU)(A;OICIID;0x10087;;;BU)");
    let directory_marked = fixture::snapshot(&directory);
    assert!(fixture::sddl(&directory).contains("A;OICIID;"));
    assert!(transaction.restore().is_err(), "inherited-marked directory authority was accepted");
    assert_eq!(transaction.cleanup_state(), CleanupState::RetryRequired);
    assert_eq!(fixture::snapshot(&file), file_resolved);
    assert_eq!(fixture::snapshot(&directory), directory_marked);
    assert_eq!(std::fs::read_dir(&tree.backup).unwrap().count(), 1);
    // Resolve only the fixture's conflicting principal authority, retaining its unrelated AU ACE.
    // Legitimate same-SID parent inheritance must not be mistaken for a plan residue.
    fixture::set(&file, "D:P(A;;FR;;;BU)(A;;FA;;;WD)(A;;FR;;;AU)");
    fixture::set(&directory, "D:P(A;OICI;FR;;;BU)(A;OICI;FA;;;WD)(A;OICI;FR;;;AU)");
    let resolved = [fixture::snapshot(&file), fixture::snapshot(&directory)];
    let outcome = transaction.restore();
    assert!(
        outcome.is_ok(),
        "retry {outcome:?}; file {}; directory {}",
        inheritance_probe::inspect(&parent, &file, false, "S-1-5-32-545"),
        inheritance_probe::inspect(&parent, &directory, true, "S-1-5-32-545")
    );
    assert!(transaction.restored());
    assert_eq!(fixture::snapshot(&file), resolved[0]);
    assert_eq!(fixture::snapshot(&directory), resolved[1]);
}

#[test]
fn native_acl_deleted_original_directory_completes_cleanup_and_releases_last_handle() {
    let _serial = fixture::serial();
    let tree = Tree::new();
    let deleted = tree.workspace.join("workspace/deleted-empty");
    std::fs::create_dir(&deleted).unwrap();
    let originals = tree.originals();
    let before = originals.iter().map(|path| fixture::snapshot(path)).collect::<Vec<_>>();
    let mut transaction = tree.plan().install(&tree.backup).unwrap();
    std::fs::remove_dir(&deleted).unwrap();
    let outcome = transaction.restore();
    let mismatches = originals
        .iter()
        .zip(&before)
        .filter_map(|(path, expected)| {
            let actual = fixture::snapshot(path);
            (actual != *expected).then_some((path, expected, actual))
        })
        .collect::<Vec<_>>();
    assert!(outcome.is_ok(), "cleanup {outcome:?}; surviving descriptor mismatches {mismatches:?}");
    assert!(mismatches.is_empty(), "surviving descriptor mismatches {mismatches:?}");
    assert!(transaction.restored());
    assert!(!deleted.exists());
    std::fs::create_dir(&deleted).unwrap();
    assert_eq!(std::fs::read_dir(&tree.backup).unwrap().count(), 0);
}

#[test]
fn native_acl_final_verification_preserves_modern_parent_and_distinct_legacy_child() {
    let _serial = fixture::serial();
    let tree = Tree::new();
    let parent = tree.workspace.join("workspace/inherited");
    let child = parent.join("nested");
    fixture::set(&parent, "D:ARAI(A;OICI;FA;;;WD)(A;OICI;FR;;;AU)");
    fixture::set(&child, "D:(A;OICI;FA;;;WD)(A;;FR;;;BA)");
    let before = [fixture::snapshot(&parent), fixture::snapshot(&child)];
    assert_ne!(fixture::control(&before[0]) & 0x0400, 0);
    assert_eq!(fixture::control(&before[1]) & 0x1400, 0);
    assert_ne!(before[0], before[1]);
    // Tree::plan supplies the nested child rule before the checked plan's outer workspace
    // rule. The intermediate parent is discovered later than the explicit child target.
    let mut transaction = tree.plan().install(&tree.backup).unwrap();
    let outcome = transaction.restore();
    let after = [fixture::snapshot(&parent), fixture::snapshot(&child)];
    assert!(outcome.is_ok(), "cleanup {outcome:?}; original {before:?}; final {after:?}");
    assert_eq!(after, before, "a later parent replay changed a previously restored child");
    assert!(transaction.restored());
    assert_eq!(std::fs::read_dir(&tree.backup).unwrap().count(), 0);
}
