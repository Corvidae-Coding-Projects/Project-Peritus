//! Acceptance rejects incomplete membership and preserves original owner-storage failures.

use super::*;
use std::error::Error as _;

#[test]
fn membership_changes_during_scan_never_publish_an_accepted_listing() {
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("first"), []).unwrap();
    let inspection =
        FolderInspection::open(&FolderIdentity::observe(source.path()).unwrap()).unwrap();
    let mut changed_membership = false;
    let result = inspection.scan_directory(None, None, |_| {
        if !changed_membership {
            std::fs::write(source.path().join("new-child"), []).unwrap();
            changed_membership = true;
        }
        Ok(())
    });
    assert_eq!(result.unwrap_err().recovery(), crate::RecoveryClass::Reobserve);
}

#[test]
fn removal_and_same_count_rename_cannot_publish_an_accepted_listing() {
    for rename in [false, true] {
        let source = tempfile::tempdir().unwrap();
        std::fs::write(source.path().join("first"), []).unwrap();
        let inspection =
            FolderInspection::open(&FolderIdentity::observe(source.path()).unwrap()).unwrap();
        let result = inspection.scan_directory(None, None, |_| {
            if rename {
                std::fs::rename(source.path().join("first"), source.path().join("second")).unwrap();
            } else {
                std::fs::remove_file(source.path().join("first")).unwrap();
            }
            Ok(())
        });
        assert_eq!(result.unwrap_err().recovery(), crate::RecoveryClass::Reobserve);
    }
}

#[test]
fn membership_passes_require_each_exact_name_once_and_ignore_iteration_order() {
    let source = tempfile::tempdir().unwrap();
    for name in ["one", "two"] {
        std::fs::write(source.path().join(name), []).unwrap();
    }
    let directory = Dir::open_ambient_dir(source.path(), cap_std::ambient_authority()).unwrap();
    let one = NativeEntryName::observed(std::ffi::OsStr::new("one")).unwrap();
    let two = NativeEntryName::observed(std::ffi::OsStr::new("two")).unwrap();
    let mut evidence = membership::DirectoryMembership::observe(&directory).unwrap();
    evidence.visit(&two).unwrap();
    evidence.visit(&one).unwrap();
    evidence.finish_pass().unwrap();
    evidence.visit(&one).unwrap();
    assert_eq!(evidence.visit(&one).unwrap_err().recovery(), crate::RecoveryClass::Reobserve);
    let mut evidence = membership::DirectoryMembership::observe(&directory).unwrap();
    evidence.visit(&one).unwrap();
    assert_eq!(evidence.finish_pass().unwrap_err().recovery(), crate::RecoveryClass::Reobserve);
    let mut evidence = membership::DirectoryMembership::observe(&directory).unwrap();
    let foreign = NativeEntryName::observed(std::ffi::OsStr::new("ONE")).unwrap();
    assert_eq!(evidence.visit(&foreign).unwrap_err().recovery(), crate::RecoveryClass::Reobserve);
}

#[test]
fn owner_storage_failure_retains_original_io_error_and_recovery_category() {
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("first"), []).unwrap();
    let inspection =
        FolderInspection::open(&FolderIdentity::observe(source.path()).unwrap()).unwrap();
    let error = inspection
        .scan_directory(None, None, |_| {
            Err(snapshot_io(std::io::Error::new(
                std::io::ErrorKind::StorageFull,
                "owner disk full",
            )))
        })
        .unwrap_err();
    assert_eq!(error.recovery(), crate::RecoveryClass::Reobserve);
    let original = error.source().unwrap().downcast_ref::<std::io::Error>().unwrap();
    assert_eq!(original.kind(), std::io::ErrorKind::StorageFull);
    assert_eq!(original.to_string(), "owner disk full");
}
