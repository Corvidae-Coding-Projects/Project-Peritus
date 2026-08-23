use crate::error::Diagnostic;
use crate::model::CargoMetadata;
use std::collections::BTreeSet;
use std::path::Path;

const REVIEWED_BUILD_SCRIPT_PACKAGES: [&str; 14] = [
    "registry+https://github.com/rust-lang/crates.io-index#curve25519-dalek@5.0.0",
    "registry+https://github.com/rust-lang/crates.io-index#getrandom@0.4.3",
    "registry+https://github.com/rust-lang/crates.io-index#libc@0.2.189",
    "registry+https://github.com/rust-lang/crates.io-index#libsqlite3-sys@0.38.2",
    "registry+https://github.com/rust-lang/crates.io-index#proc-macro2@1.0.107",
    "registry+https://github.com/rust-lang/crates.io-index#quote@1.0.47",
    "registry+https://github.com/rust-lang/crates.io-index#rustix@1.1.4",
    "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.229",
    "registry+https://github.com/rust-lang/crates.io-index#serde_core@1.0.229",
    "registry+https://github.com/rust-lang/crates.io-index#serde_json@1.0.149",
    "git+https://github.com/verus-lang/verus.git?rev=92f466f247f45128c630d1c843fd6e27d2115587#verus_prettyplease@0.0.0-2026-08-09-0044",
    "git+https://github.com/verus-lang/verus.git?rev=92f466f247f45128c630d1c843fd6e27d2115587#verus_syn@0.0.0-2026-08-02-0125",
    "git+https://github.com/verus-lang/verus.git?rev=92f466f247f45128c630d1c843fd6e27d2115587#vstd@0.0.0-2026-08-09-0044",
    "registry+https://github.com/rust-lang/crates.io-index#zmij@1.0.23",
];

const REVIEWED_PROC_MACRO_PACKAGES: [&str; 4] = [
    "registry+https://github.com/rust-lang/crates.io-index#curve25519-dalek-derive@0.1.1",
    "registry+https://github.com/rust-lang/crates.io-index#serde_derive@1.0.229",
    "git+https://github.com/verus-lang/verus.git?rev=92f466f247f45128c630d1c843fd6e27d2115587#verus_builtin_macros@0.0.0-2026-08-09-0044",
    "git+https://github.com/verus-lang/verus.git?rev=92f466f247f45128c630d1c843fd6e27d2115587#verus_state_machines_macros@0.0.0-2026-08-02-0125",
];

pub(super) fn validate(root: &Path, cargo: &CargoMetadata, diagnostics: &mut Vec<Diagnostic>) {
    let workspace: BTreeSet<_> = cargo.workspace_members.iter().map(String::as_str).collect();
    for package in cargo.packages.iter().filter(|package| !workspace.contains(package.id.as_str()))
    {
        let has_build_script = package
            .targets
            .iter()
            .any(|target| target.kind.iter().any(|kind| kind == "custom-build"));
        if has_build_script && !REVIEWED_BUILD_SCRIPT_PACKAGES.contains(&package.id.as_str()) {
            diagnostics.push(Diagnostic::at(
                package.manifest_path.strip_prefix(root).unwrap_or(&package.manifest_path),
                format!(
                    "dependency package `{}` has an unreviewed executable build script ({})",
                    package.name, package.id
                ),
                "remove the dependency or add its exact immutable package identity only after reviewing the executed build script and updating proof-impact evidence",
            ));
        }

        let is_proc_macro = package.targets.iter().any(|target| {
            target.kind.iter().chain(&target.crate_types).any(|kind| kind == "proc-macro")
        });
        if is_proc_macro && !REVIEWED_PROC_MACRO_PACKAGES.contains(&package.id.as_str()) {
            diagnostics.push(Diagnostic::at(
                package.manifest_path.strip_prefix(root).unwrap_or(&package.manifest_path),
                format!(
                    "dependency package `{}` has an unreviewed executable procedural macro ({})",
                    package.name, package.id
                ),
                "remove the dependency or add its exact immutable package identity only after reviewing token generation and updating proof-impact evidence",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
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
                kind: vec![kind.to_owned()],
                crate_types: vec![kind.to_owned()],
                src_path: PathBuf::from(format!("/registry/{name}/entry.rs")),
            }],
            metadata: CargoPackageMetadata::default(),
        }
    }

    #[test]
    fn rejects_unreviewed_dependency_build_scripts_and_proc_macros() {
        let cargo = CargoMetadata {
            packages: vec![
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
        let cargo = CargoMetadata {
            packages: vec![
                package(
                    "registry+https://github.com/rust-lang/crates.io-index#curve25519-dalek@5.0.0",
                    "curve25519-dalek",
                    "custom-build",
                ),
                package(
                    "registry+https://github.com/rust-lang/crates.io-index#curve25519-dalek-derive@0.1.1",
                    "curve25519-dalek-derive",
                    "proc-macro",
                ),
                package(
                    "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.229",
                    "serde",
                    "custom-build",
                ),
                package(
                    "registry+https://github.com/rust-lang/crates.io-index#getrandom@0.4.3",
                    "getrandom",
                    "custom-build",
                ),
                package(
                    "registry+https://github.com/rust-lang/crates.io-index#libsqlite3-sys@0.38.2",
                    "libsqlite3-sys",
                    "custom-build",
                ),
                package(
                    "registry+https://github.com/rust-lang/crates.io-index#rustix@1.1.4",
                    "rustix",
                    "custom-build",
                ),
                package(
                    "registry+https://github.com/rust-lang/crates.io-index#serde_derive@1.0.229",
                    "serde_derive",
                    "proc-macro",
                ),
            ],
            workspace_members: Vec::new(),
        };
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
}
