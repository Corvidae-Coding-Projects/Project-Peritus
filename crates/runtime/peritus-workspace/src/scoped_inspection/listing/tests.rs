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
