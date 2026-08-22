//! Boundary tests for the public verification-manifest path inventory.

use peritus_tcb::{actors_manifest_path, verification_manifest_paths};
use std::collections::BTreeSet;
use std::path::Path;

#[test]
fn manifest_paths_are_unique_repository_relative_toml_files() {
    let paths = verification_manifest_paths();
    assert_eq!(paths[0], actors_manifest_path());
    let unique: BTreeSet<_> = paths.into_iter().collect();
    assert_eq!(unique.len(), paths.len());

    for path in paths {
        let path = Path::new(path);
        assert!(!path.is_absolute());
        assert_eq!(path.extension().and_then(|extension| extension.to_str()), Some("toml"));
        assert_eq!(path.parent(), Some(Path::new("verification")));
    }
}

#[test]
fn manifest_paths_resolve_to_regular_repository_files() {
    let package_root =
        std::env::current_dir().expect("Cargo must provide a test working directory");
    let repository_root = package_root.join("../../..");

    for relative in verification_manifest_paths() {
        let metadata = repository_root
            .join(relative)
            .symlink_metadata()
            .unwrap_or_else(|error| panic!("{relative} must be inspectable: {error}"));
        assert!(metadata.file_type().is_file(), "{relative} must be a regular file");
    }
}
