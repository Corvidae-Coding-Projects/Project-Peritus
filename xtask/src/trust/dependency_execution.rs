use crate::error::Diagnostic;
use crate::model::CargoMetadata;
use std::collections::BTreeSet;
use std::path::Path;

const REVIEWED_BUILD_SCRIPT_PACKAGES: [&str; 65] = [
    "registry+https://github.com/rust-lang/crates.io-index#alloca@0.4.0",
    "registry+https://github.com/rust-lang/crates.io-index#anyhow@1.0.104",
    "registry+https://github.com/rust-lang/crates.io-index#async-io@2.6.0",
    "registry+https://github.com/rust-lang/crates.io-index#aws-lc-rs@1.18.0",
    "registry+https://github.com/rust-lang/crates.io-index#aws-lc-sys@0.44.0",
    "registry+https://github.com/rust-lang/crates.io-index#cap-fs-ext@4.0.3",
    "registry+https://github.com/rust-lang/crates.io-index#cap-primitives@4.0.3",
    "registry+https://github.com/rust-lang/crates.io-index#cap-std@4.0.3",
    "registry+https://github.com/rust-lang/crates.io-index#crossbeam-utils@0.8.22",
    "registry+https://github.com/rust-lang/crates.io-index#crc32fast@1.5.1",
    "registry+https://github.com/rust-lang/crates.io-index#crunchy@0.2.4",
    "registry+https://github.com/rust-lang/crates.io-index#curve25519-dalek@5.0.0",
    "registry+https://github.com/rust-lang/crates.io-index#generic-array@0.14.7",
    "registry+https://github.com/rust-lang/crates.io-index#getrandom@0.4.3",
    "registry+https://github.com/rust-lang/crates.io-index#httparse@1.10.1",
    "registry+https://github.com/rust-lang/crates.io-index#icu_normalizer_data@2.3.0",
    "registry+https://github.com/rust-lang/crates.io-index#icu_properties_data@2.3.0",
    "registry+https://github.com/rust-lang/crates.io-index#instability@0.3.13",
    "registry+https://github.com/rust-lang/crates.io-index#io-extras@0.19.0",
    "registry+https://github.com/rust-lang/crates.io-index#io-lifetimes@2.0.4",
    "registry+https://github.com/rust-lang/crates.io-index#io-lifetimes@3.0.1",
    "registry+https://github.com/rust-lang/crates.io-index#jni@0.22.4",
    "registry+https://github.com/rust-lang/crates.io-index#jni-macros@0.22.4",
    "registry+https://github.com/rust-lang/crates.io-index#libc@0.2.189",
    "registry+https://github.com/rust-lang/crates.io-index#libm@0.2.16",
    "registry+https://github.com/rust-lang/crates.io-index#libsqlite3-sys@0.38.2",
    "registry+https://github.com/rust-lang/crates.io-index#memoffset@0.9.1",
    "registry+https://github.com/rust-lang/crates.io-index#nix@0.28.0",
    "registry+https://github.com/rust-lang/crates.io-index#nix@0.31.3",
    "registry+https://github.com/rust-lang/crates.io-index#num-traits@0.2.19",
    "registry+https://github.com/rust-lang/crates.io-index#parking_lot_core@0.9.12",
    "registry+https://github.com/rust-lang/crates.io-index#portable-atomic@1.15.0",
    "registry+https://github.com/rust-lang/crates.io-index#proc-macro2@1.0.107",
    "registry+https://github.com/rust-lang/crates.io-index#quote@1.0.47",
    "registry+https://github.com/rust-lang/crates.io-index#quinn@0.11.11",
    "registry+https://github.com/rust-lang/crates.io-index#quinn-udp@0.5.15",
    "registry+https://github.com/rust-lang/crates.io-index#ring@0.17.14",
    "registry+https://github.com/rust-lang/crates.io-index#rustix@1.1.4",
    "registry+https://github.com/rust-lang/crates.io-index#rustls@0.23.43",
    "registry+https://github.com/rust-lang/crates.io-index#rustversion@1.0.23",
    "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.229",
    "registry+https://github.com/rust-lang/crates.io-index#serde_core@1.0.229",
    "registry+https://github.com/rust-lang/crates.io-index#serde_json@1.0.149",
    "registry+https://github.com/rust-lang/crates.io-index#signal-hook@0.3.18",
    "registry+https://github.com/rust-lang/crates.io-index#thiserror@1.0.69",
    "registry+https://github.com/rust-lang/crates.io-index#thiserror@2.0.20",
    "registry+https://github.com/rust-lang/crates.io-index#wasm-bindgen@0.2.127",
    "registry+https://github.com/rust-lang/crates.io-index#wasm-bindgen-shared@0.2.127",
    "registry+https://github.com/rust-lang/crates.io-index#web_atoms@0.2.6",
    "git+https://github.com/verus-lang/verus.git?rev=92f466f247f45128c630d1c843fd6e27d2115587#verus_prettyplease@0.0.0-2026-08-09-0044",
    "git+https://github.com/verus-lang/verus.git?rev=92f466f247f45128c630d1c843fd6e27d2115587#verus_syn@0.0.0-2026-08-02-0125",
    "git+https://github.com/verus-lang/verus.git?rev=92f466f247f45128c630d1c843fd6e27d2115587#vstd@0.0.0-2026-08-09-0044",
    "registry+https://github.com/rust-lang/crates.io-index#winapi@0.3.9",
    "registry+https://github.com/rust-lang/crates.io-index#winapi-i686-pc-windows-gnu@0.4.0",
    "registry+https://github.com/rust-lang/crates.io-index#winapi-x86_64-pc-windows-gnu@0.4.0",
    "registry+https://github.com/rust-lang/crates.io-index#windows_aarch64_gnullvm@0.52.6",
    "registry+https://github.com/rust-lang/crates.io-index#windows_aarch64_msvc@0.52.6",
    "registry+https://github.com/rust-lang/crates.io-index#windows_i686_gnu@0.52.6",
    "registry+https://github.com/rust-lang/crates.io-index#windows_i686_gnullvm@0.52.6",
    "registry+https://github.com/rust-lang/crates.io-index#windows_i686_msvc@0.52.6",
    "registry+https://github.com/rust-lang/crates.io-index#windows_x86_64_gnu@0.52.6",
    "registry+https://github.com/rust-lang/crates.io-index#windows_x86_64_gnullvm@0.52.6",
    "registry+https://github.com/rust-lang/crates.io-index#windows_x86_64_msvc@0.52.6",
    "registry+https://github.com/rust-lang/crates.io-index#winreg@0.10.1",
    "registry+https://github.com/rust-lang/crates.io-index#zmij@1.0.23",
];

const REVIEWED_PROC_MACRO_PACKAGES: [&str; 32] = [
    "registry+https://github.com/rust-lang/crates.io-index#async-recursion@1.1.1",
    "registry+https://github.com/rust-lang/crates.io-index#async-trait@0.1.92",
    "registry+https://github.com/rust-lang/crates.io-index#curve25519-dalek-derive@0.1.1",
    "registry+https://github.com/rust-lang/crates.io-index#darling_macro@0.24.1",
    "registry+https://github.com/rust-lang/crates.io-index#derive_more-impl@2.1.1",
    "registry+https://github.com/rust-lang/crates.io-index#displaydoc@0.2.7",
    "registry+https://github.com/rust-lang/crates.io-index#document-features@0.2.12",
    "registry+https://github.com/rust-lang/crates.io-index#enumflags2_derive@0.7.12",
    "registry+https://github.com/rust-lang/crates.io-index#futures-macro@0.3.34",
    "registry+https://github.com/rust-lang/crates.io-index#indoc@2.0.7",
    "registry+https://github.com/rust-lang/crates.io-index#instability@0.3.13",
    "registry+https://github.com/rust-lang/crates.io-index#jni-macros@0.22.4",
    "registry+https://github.com/rust-lang/crates.io-index#jni-sys-macros@0.4.1",
    "registry+https://github.com/rust-lang/crates.io-index#palette_derive@0.7.7",
    "registry+https://github.com/rust-lang/crates.io-index#rustversion@1.0.23",
    "registry+https://github.com/rust-lang/crates.io-index#serde_derive@1.0.229",
    "registry+https://github.com/rust-lang/crates.io-index#serde_repr@0.1.21",
    "registry+https://github.com/rust-lang/crates.io-index#strum_macros@0.28.0",
    "registry+https://github.com/rust-lang/crates.io-index#thiserror-impl@1.0.69",
    "registry+https://github.com/rust-lang/crates.io-index#thiserror-impl@2.0.20",
    "registry+https://github.com/rust-lang/crates.io-index#tokio-macros@2.7.2",
    "registry+https://github.com/rust-lang/crates.io-index#tracing-attributes@0.1.31",
    "registry+https://github.com/rust-lang/crates.io-index#wasm-bindgen-macro@0.2.127",
    "registry+https://github.com/rust-lang/crates.io-index#zbus_macros@5.19.0",
    "registry+https://github.com/rust-lang/crates.io-index#zvariant_derive@5.15.0",
    "git+https://github.com/verus-lang/verus.git?rev=92f466f247f45128c630d1c843fd6e27d2115587#verus_builtin_macros@0.0.0-2026-08-09-0044",
    "git+https://github.com/verus-lang/verus.git?rev=92f466f247f45128c630d1c843fd6e27d2115587#verus_state_machines_macros@0.0.0-2026-08-02-0125",
    "registry+https://github.com/rust-lang/crates.io-index#windows-implement@0.60.2",
    "registry+https://github.com/rust-lang/crates.io-index#windows-interface@0.59.3",
    "registry+https://github.com/rust-lang/crates.io-index#yoke-derive@0.8.2",
    "registry+https://github.com/rust-lang/crates.io-index#zerofrom-derive@0.1.7",
    "registry+https://github.com/rust-lang/crates.io-index#zerovec-derive@0.11.6",
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
#[path = "dependency_execution_tests.rs"]
mod tests;
