//! Native descriptor format and exact replay checks against an independent path oracle.

use super::test_fixture as fixture;

use super::{AclObject, descriptor::Dacl};

#[test]
fn exact_descriptor_replay_preserves_null_empty_legacy_and_protected_dacls() {
    let _serial = fixture::serial();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("descriptor");
    std::fs::write(&path, b"fixture").unwrap();
    let object = AclObject::capture(&path).unwrap();
    for original in ["D:NO_ACCESS_CONTROL", "D:", "D:(A;;FA;;;WD)", "D:P(A;;FA;;;WD)"] {
        fixture::set(&path, original);
        let expected = fixture::snapshot(&path);
        let saved = Dacl::read(&object.file).unwrap();
        fixture::set(&path, "D:AI(A;;FA;;;WD)(A;ID;FR;;;BU)");
        saved.restore_exact(&object.file).unwrap();
        assert_eq!(fixture::snapshot(&path), expected, "DACL state changed: {original}");
        assert_eq!(fixture::control(&fixture::snapshot(&path)), fixture::control(&expected));
        assert!(fixture::sddl(&path).starts_with("D:"));
    }
}
