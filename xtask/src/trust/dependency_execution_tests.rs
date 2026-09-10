use super::validate;
use crate::model::{CargoMetadata, CargoPackage, CargoPackageMetadata, CargoTarget};
use std::path::{Path, PathBuf};

fn package(id: &str, name: &str, kind: &str) -> CargoPackage {
    CargoPackage {
        id: id.to_owned(),
        name: name.to_owned(),
        version: "1.0.0".to_owned(),
        edition: "2024".to_owned(),
        rust_version: Some("1.97.1".to_owned()),
        license: Some("MIT".to_owned()),
        manifest_path: PathBuf::from(format!("/registry/{name}/Cargo.toml")),
        readme: None,
        dependencies: Vec::new(),
        targets: vec![CargoTarget {
            name: name.to_owned(),
            kind: vec![kind.to_owned()],
            crate_types: vec![kind.to_owned()],
            src_path: PathBuf::from(format!("/registry/{name}/entry.rs")),
        }],
        metadata: CargoPackageMetadata::default(),
    }
}

fn package_from_id(id: &str, kind: &str) -> CargoPackage {
    let (_, versioned_name) = id.rsplit_once('#').expect("package ID must contain a source");
    let (name, _) = versioned_name.rsplit_once('@').expect("package ID must contain a version");
    package(id, name, kind)
}

#[test]
fn rejects_unreviewed_dependency_build_scripts_and_proc_macros() {
    let cargo = CargoMetadata {
        packages: vec![
            package(
                "registry+https://github.com/rust-lang/crates.io-index#anyhow@1.0.104",
                "anyhow",
                "custom-build",
            ),
            package(
                "registry+https://github.com/rust-lang/crates.io-index#surprise-build@1.0.0",
                "surprise-build",
                "custom-build",
            ),
            package(
                "registry+https://github.com/rust-lang/crates.io-index#surprise-macro@1.0.0",
                "surprise-macro",
                "proc-macro",
            ),
        ],
        workspace_members: Vec::new(),
    };
    let mut diagnostics = Vec::new();

    validate(Path::new("/workspace"), &cargo, &mut diagnostics);

    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.iter().any(|item| item.message().contains("build script")));
    assert!(diagnostics.iter().any(|item| item.message().contains("procedural macro")));
}

#[test]
fn accepts_only_exact_reviewed_executable_dependency_identities() {
    let build_scripts = [
        "registry+https://github.com/rust-lang/crates.io-index#curve25519-dalek@5.0.0",
        "registry+https://github.com/rust-lang/crates.io-index#cap-fs-ext@4.0.3",
        "registry+https://github.com/rust-lang/crates.io-index#cap-primitives@4.0.3",
        "registry+https://github.com/rust-lang/crates.io-index#cap-std@4.0.3",
        "registry+https://github.com/rust-lang/crates.io-index#crc32fast@1.5.1",
        "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.229",
        "registry+https://github.com/rust-lang/crates.io-index#getrandom@0.4.3",
        "registry+https://github.com/rust-lang/crates.io-index#libsqlite3-sys@0.38.2",
        "registry+https://github.com/rust-lang/crates.io-index#io-extras@0.19.0",
        "registry+https://github.com/rust-lang/crates.io-index#io-lifetimes@2.0.4",
        "registry+https://github.com/rust-lang/crates.io-index#io-lifetimes@3.0.1",
        "registry+https://github.com/rust-lang/crates.io-index#nix@0.28.0",
        "registry+https://github.com/rust-lang/crates.io-index#nix@0.31.3",
        "registry+https://github.com/rust-lang/crates.io-index#rustix@1.1.4",
        "registry+https://github.com/rust-lang/crates.io-index#thiserror@1.0.69",
        "registry+https://github.com/rust-lang/crates.io-index#web_atoms@0.2.6",
        "registry+https://github.com/rust-lang/crates.io-index#winapi@0.3.9",
        "registry+https://github.com/rust-lang/crates.io-index#winapi-i686-pc-windows-gnu@0.4.0",
        "registry+https://github.com/rust-lang/crates.io-index#winapi-x86_64-pc-windows-gnu@0.4.0",
        "registry+https://github.com/rust-lang/crates.io-index#winreg@0.10.1",
    ];
    let proc_macros = [
        "registry+https://github.com/rust-lang/crates.io-index#curve25519-dalek-derive@0.1.1",
        "registry+https://github.com/rust-lang/crates.io-index#serde_derive@1.0.229",
        "registry+https://github.com/rust-lang/crates.io-index#thiserror-impl@1.0.69",
        "registry+https://github.com/rust-lang/crates.io-index#windows-implement@0.60.2",
        "registry+https://github.com/rust-lang/crates.io-index#windows-interface@0.59.3",
    ];
    let packages = build_scripts
        .into_iter()
        .map(|id| package_from_id(id, "custom-build"))
        .chain(proc_macros.into_iter().map(|id| package_from_id(id, "proc-macro")))
        .collect();
    let cargo = CargoMetadata { packages, workspace_members: Vec::new() };
    let mut diagnostics = Vec::new();

    validate(Path::new("/workspace"), &cargo, &mut diagnostics);

    assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
}

#[test]
fn rejects_version_and_source_near_misses_for_cryptographic_execution_dependencies() {
    let cargo = CargoMetadata {
        packages: vec![
            package(
                "registry+https://github.com/rust-lang/crates.io-index#curve25519-dalek@5.0.1",
                "curve25519-dalek",
                "custom-build",
            ),
            package(
                "registry+https://github.com/rust-lang/crates.io-index#curve25519-dalek-derive@0.1.2",
                "curve25519-dalek-derive",
                "proc-macro",
            ),
            package(
                "registry+https://registry.example.invalid/index#curve25519-dalek@5.0.0",
                "curve25519-dalek",
                "custom-build",
            ),
            package(
                "git+https://example.invalid/curve25519-dalek?rev=0123456789012345678901234567890123456789#curve25519-dalek@5.0.0",
                "curve25519-dalek",
                "custom-build",
            ),
            package(
                "path+file:///tmp/curve25519-dalek#curve25519-dalek@5.0.0",
                "curve25519-dalek",
                "custom-build",
            ),
            package(
                "registry+https://registry.example.invalid/index#curve25519-dalek-derive@0.1.1",
                "curve25519-dalek-derive",
                "proc-macro",
            ),
            package(
                "git+https://example.invalid/curve25519-dalek?rev=0123456789012345678901234567890123456789#curve25519-dalek-derive@0.1.1",
                "curve25519-dalek-derive",
                "proc-macro",
            ),
            package(
                "path+file:///tmp/curve25519-dalek-derive#curve25519-dalek-derive@0.1.1",
                "curve25519-dalek-derive",
                "proc-macro",
            ),
        ],
        workspace_members: Vec::new(),
    };
    let mut diagnostics = Vec::new();

    validate(Path::new("/workspace"), &cargo, &mut diagnostics);

    assert_eq!(diagnostics.len(), 8);
    assert_eq!(
        diagnostics.iter().filter(|item| item.message().contains("build script")).count(),
        4
    );
    assert_eq!(
        diagnostics.iter().filter(|item| item.message().contains("procedural macro")).count(),
        4
    );
}

#[test]
fn rejects_version_and_source_near_misses_for_new_build_scripts() {
    let identities = [
        ("cap-fs-ext", "4.0.3", "4.0.4"),
        ("cap-primitives", "4.0.3", "4.0.4"),
        ("cap-std", "4.0.3", "4.0.4"),
        ("crc32fast", "1.5.1", "1.5.2"),
        ("io-extras", "0.19.0", "0.19.1"),
        ("io-lifetimes", "2.0.4", "2.0.5"),
        ("io-lifetimes", "3.0.1", "3.0.2"),
    ];
    let mut packages = Vec::new();
    for (name, reviewed, near_miss) in identities {
        packages.push(package(
            &format!("registry+https://github.com/rust-lang/crates.io-index#{name}@{near_miss}"),
            name,
            "custom-build",
        ));
        packages.push(package(
            &format!("registry+https://registry.example.invalid/index#{name}@{reviewed}"),
            name,
            "custom-build",
        ));
    }
    let cargo = CargoMetadata { packages, workspace_members: Vec::new() };
    let mut diagnostics = Vec::new();

    validate(Path::new("/workspace"), &cargo, &mut diagnostics);

    assert_eq!(diagnostics.len(), identities.len() * 2);
    assert!(diagnostics.iter().all(|item| item.message().contains("build script")));
}
