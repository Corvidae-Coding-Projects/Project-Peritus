use std::path::Path;

use super::bounded_path;

#[test]
fn preserves_existing_paths_through_each_platform_boundary() {
    for limit in [103, 107] {
        let original = format!("/{}.sock", "a".repeat(limit - 6));
        assert_eq!(original.len(), limit);
        assert_eq!(
            bounded_path(Path::new(&original), limit).expect("usable path"),
            Path::new(&original)
        );
        let oversized = format!("/{original}");
        let selected = bounded_path(Path::new(&oversized), limit).expect("runtime path");
        assert_ne!(selected, Path::new(&oversized));
        assert!(selected.as_os_str().len() < 103);
    }
}

#[test]
fn long_unicode_paths_are_stable_and_isolated_by_the_complete_original_path() {
    let original = format!("/Users/{}/State/peritus-0123.sock", "工作🦀".repeat(60));
    let first = bounded_path(Path::new(&original), 103).expect("long path");
    assert_eq!(first, bounded_path(Path::new(&original), 103).expect("repeat"));
    assert!(first.as_os_str().len() <= 103);
    for different in [original.replace("State", "OtherState"), original.replace("0123", "4567")] {
        assert_ne!(first, bounded_path(Path::new(&different), 103).expect("different endpoint"));
    }
}

#[test]
fn rejects_invalid_paths_without_turning_them_into_valid_runtime_names() {
    for path in ["relative.sock", "/", "/root/", "/root/.", "/root/..", "/root/\0sock"] {
        assert_eq!(
            bounded_path(Path::new(path), 103).expect_err("invalid path").kind(),
            std::io::ErrorKind::InvalidInput
        );
    }
    assert!(bounded_path(Path::new("/an/ordinary/socket.sock"), 1).is_err());
}
