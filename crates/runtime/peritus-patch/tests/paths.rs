//! Path capacity is independent of authority and host filesystem support.

use peritus_patch::WorkspacePath;

#[test]
fn component_capacity_is_owned_by_the_actual_filesystem() {
    let path = "x".repeat(256);
    assert_eq!(WorkspacePath::new(&path).unwrap().as_str(), path);
}

#[test]
fn complete_path_capacity_has_no_synthetic_byte_ceiling() {
    let path = vec!["x".repeat(200); 21].join("/");
    assert!(path.len() > 4_096);
    assert_eq!(WorkspacePath::new(&path).unwrap().as_str(), path);
}

#[test]
fn component_depth_has_no_synthetic_ceiling() {
    let path = vec!["x"; 257].join("/");
    assert_eq!(WorkspacePath::new(&path).unwrap().as_str(), path);
}

#[cfg(unix)]
#[test]
fn unix_names_do_not_inherit_windows_alias_rules() {
    for path in ["a:b", "a\\b", "name.", "name ", "NUL", "line\nbreak"] {
        assert_eq!(WorkspacePath::new(path).unwrap().as_str(), path);
    }
}

#[test]
fn path_capacity_does_not_grant_traversal_or_protected_metadata() {
    for path in [
        "",
        "/absolute",
        "a/",
        "a//b",
        "a/../b",
        "a/./b",
        "a\0b",
        ".git/config",
        "x/.peritus/state",
        ".peritus-txn-owned/data",
    ] {
        assert!(WorkspacePath::new(path).is_err(), "accepted {path:?}");
    }
}
