use super::{check, controlled_kind, inspect_controlled_path};
use crate::error::{Diagnostic, ErrorCode};
use crate::model::{
    ArchitecturePolicy, CargoMetadata, CargoPackage, CargoPackageMetadata, CargoTarget,
    ControlledSourceKind,
};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};

use super::crate_root::{RootKind, inspect_crate_root};

static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let id = FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!("peritus-xtask-source-{}-{id}", process::id()));
        fs::create_dir_all(&path).expect("fixture directory must be created");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _cleanup_result = fs::remove_dir_all(&self.0);
    }
}

fn inspect(path: &str, source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let root_kind = if path.ends_with("main.rs") { RootKind::Binary } else { RootKind::Library };
    inspect_crate_root(Path::new(path), source, 80, root_kind, &mut diagnostics);
    diagnostics
}

fn policy() -> ArchitecturePolicy {
    toml::from_str(include_str!("../../../architecture.toml")).expect("policy must parse")
}

fn cargo(root: &Path, targets: Vec<CargoTarget>) -> CargoMetadata {
    let id = "fixture-xtask".to_owned();
    CargoMetadata {
        packages: vec![CargoPackage {
            id: id.clone(),
            name: "xtask".to_owned(),
            version: "0.0.0".to_owned(),
            edition: "2024".to_owned(),
            rust_version: Some("1.97.1".to_owned()),
            license: Some("MIT".to_owned()),
            manifest_path: root.join("xtask/Cargo.toml"),
            readme: Some(PathBuf::from("README.md")),
            dependencies: Vec::new(),
            targets,
            metadata: CargoPackageMetadata::default(),
        }],
        workspace_members: vec![id],
    }
}

fn target(root: &Path, relative: &str, kind: &str) -> CargoTarget {
    CargoTarget {
        kind: vec![kind.to_owned()],
        crate_types: vec![kind.to_owned()],
        src_path: root.join(relative),
    }
}

fn write(fixture: &TestDirectory, relative: &str, contents: &str) {
    let path = fixture.path().join(relative);
    fs::create_dir_all(path.parent().expect("source must have a parent"))
        .expect("source directory must be created");
    fs::write(path, contents).expect("source fixture must be written");
}

fn padded_source(line_count: usize, final_line: &str) -> String {
    let mut source = "// padding\n".repeat(line_count.saturating_sub(1));
    source.push_str(final_line);
    source
}

#[test]
fn crate_root_rejects_pub_crate_type_bypass() {
    let diagnostics = inspect("sample/src/lib.rs", "pub(crate) struct DomainState;\n");
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message().contains("implementation"));
}

#[test]
fn crate_root_rejects_qualified_functions_and_all_item_definitions() {
    let prohibited = [
        "pub(crate) async fn work() { perform(); }",
        "const unsafe extern \"C\" fn work() { perform(); }",
        "pub(crate) type Name = u64;",
        "impl Name {}",
        "unsafe trait Boundary {}",
        "macro_rules! build { () => { 1 } }",
        "pub(crate) static VALUE: u8 = 1;",
        "pub(crate) const VALUE: u8 = 1;",
        "pub(crate) enum State { Ready }",
        "pub(crate) union Bits { value: u8 }",
        "pub(crate) mod inline { pub fn work() { perform(); } }",
    ];
    for source in prohibited {
        assert!(
            !inspect("sample/src/lib.rs", source).is_empty(),
            "crate-root item unexpectedly allowed: {source}"
        );
    }
}

#[test]
fn crate_root_allows_attributes_imports_declarations_and_reexports() {
    let source = r#"
        #![doc = "composition"]
        use std::{path::Path, sync::Arc};
        pub(crate) mod engine;
        #[cfg(unix)]
        mod platform;
        pub use engine::{Command, Error};
        extern crate core;
    "#;
    assert!(inspect("sample/src/lib.rs", source).is_empty());
}

#[test]
fn crate_root_allows_only_composition_inside_the_verus_wrapper() {
    let composition = r"
        use vstd::prelude::*;
        verus! {
            mod state;
            pub use state::{Command, Error};
        }
    ";
    assert!(inspect("sample/src/lib.rs", composition).is_empty());

    for prohibited in [
        "verus! { pub struct Hidden; }",
        "verus! { verus! { mod nested; } }",
        "other! { mod hidden; }",
    ] {
        assert!(
            !inspect("sample/src/lib.rs", prohibited).is_empty(),
            "crate-root macro unexpectedly allowed: {prohibited}"
        );
    }
}

#[test]
fn minimal_binary_root_is_allowed_but_qualified_main_is_not() {
    let source = "use std::process::ExitCode;\nfn main() -> ExitCode { app::run() }\n";
    assert!(inspect("sample/src/main.rs", source).is_empty());
    assert!(!inspect("sample/src/main.rs", "async fn main() { app::run(); }\n").is_empty());
}

#[test]
fn comments_and_literals_cannot_hide_or_invent_items() {
    let source = r##"
        //! `struct Example` is documentation, not an item.
        #[doc = r#"fn documented() { compose(); }"#]
        pub mod api;
        /* nested /* pub(crate) struct Hidden; */ comment */
    "##;
    assert!(inspect("sample/src/lib.rs", source).is_empty());
}

#[test]
fn generated_and_schema_paths_require_matching_owned_roots() {
    assert_eq!(
        controlled_kind(Path::new("crates/api/schema/generated/types.rs")),
        Some(ControlledSourceKind::GeneratedSchema)
    );
    let mut policy: ArchitecturePolicy =
        toml::from_str(include_str!("../../../architecture.toml")).expect("policy must parse");
    let mut diagnostics = Vec::new();
    inspect_controlled_path(
        Path::new("crates/api/schema/types.proto"),
        false,
        &policy,
        &mut diagnostics,
    );
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message().contains("no reviewed")));

    policy.controlled_source_roots.push(crate::model::ControlledSourceRoot {
        path: "crates/api/schema".into(),
        owner: "A3".to_owned(),
        kind: ControlledSourceKind::Schema,
        rationale: "A3 owns protocol compatibility and schema evolution.".to_owned(),
    });
    diagnostics.clear();
    inspect_controlled_path(
        Path::new("crates/api/schema/types.proto"),
        false,
        &policy,
        &mut diagnostics,
    );
    assert!(diagnostics.is_empty());
}

#[test]
fn source_layout_preserves_unowned_crates_diagnostic() {
    let fixture = TestDirectory::new();
    let source = fixture.path().join("crates/unowned/src/lib.rs");
    fs::create_dir_all(source.parent().expect("source must have a parent"))
        .expect("source directory must be created");
    fs::write(&source, "pub mod api;\n").expect("source fixture must be written");
    let policy = policy();
    let cargo = cargo(fixture.path(), Vec::new());

    let error =
        check(fixture.path(), &policy, &cargo).expect_err("unowned crates source must fail");
    assert_eq!(error.code(), ErrorCode::SourceLayout);
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path() == Some(Path::new("crates/unowned/src/lib.rs"))
            && diagnostic.message().contains("no registered package owner")
    }));
}

#[test]
fn metadata_declared_non_rust_lib_and_bin_roots_enforce_layout() {
    let fixture = TestDirectory::new();
    write(&fixture, "xtask/src/lib.txt", &padded_source(428, "pub struct Hidden;\n"));
    write(&fixture, "xtask/src/bin.txt", &padded_source(428, "fn main() { let _ = (); }\n"));
    let cargo = cargo(
        fixture.path(),
        vec![
            target(fixture.path(), "xtask/src/lib.txt", "lib"),
            target(fixture.path(), "xtask/src/bin.txt", "bin"),
        ],
    );

    let error = check(fixture.path(), &policy(), &cargo)
        .expect_err("metadata-declared roots must enforce budgets and composition");
    for path in ["xtask/src/lib.txt", "xtask/src/bin.txt"] {
        assert!(error.diagnostics().iter().any(|diagnostic| {
            diagnostic.path() == Some(Path::new(path))
                && diagnostic.message().contains("source has 428 lines")
        }));
        assert!(error.diagnostics().iter().any(|diagnostic| {
            diagnostic.path() == Some(Path::new(path))
                && diagnostic.message().contains("crate root has 428 lines")
        }));
    }
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path() == Some(Path::new("xtask/src/lib.txt"))
            && diagnostic.message().contains("implementation")
    }));
    assert!(!error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path() == Some(Path::new("xtask/src/bin.txt"))
            && diagnostic.message().contains("implementation")
    }));
}

#[test]
fn non_rust_includes_enforce_budget_module_name_and_origin_ownership() {
    let fixture = TestDirectory::new();
    write(
        &fixture,
        "xtask/src/lib.txt",
        "include!(\"helpers\");\ninclude!(\"../../detached/foreign.inc\");\n",
    );
    write(&fixture, "xtask/src/helpers", &"// included\n".repeat(401));
    write(&fixture, "detached/foreign.inc", "fn foreign() { let _ = (); }\n");
    let cargo = cargo(fixture.path(), vec![target(fixture.path(), "xtask/src/lib.txt", "lib")]);

    let error = check(fixture.path(), &policy(), &cargo)
        .expect_err("referenced non-Rust inputs must be owned and layout-checked");
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path() == Some(Path::new("xtask/src/helpers"))
            && diagnostic.message().contains("source has 401 lines")
    }));
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path() == Some(Path::new("xtask/src/helpers"))
            && diagnostic.message().contains("generic module name `helpers`")
    }));
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path() == Some(Path::new("xtask/src/lib.txt"))
            && diagnostic.message().contains("outside originating package `xtask`")
    }));
}
