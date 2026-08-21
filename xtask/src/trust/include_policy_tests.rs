use super::check_fixture as check;
use crate::error::ErrorCode;
use crate::model::{ArchitecturePolicy, PackagePolicy};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let id = FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir()
            .join(format!("peritus-xtask-trust-include-policy-{}-{id}", process::id()));
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

fn policy() -> ArchitecturePolicy {
    ArchitecturePolicy {
        schema: 1,
        soft_source_lines: 400,
        hard_source_lines: 700,
        root_module_lines: 80,
        required_license: "MIT".to_owned(),
        ignored_directories: Vec::new(),
        forbidden_module_names: Vec::new(),
        trusted_source_roots: Vec::new(),
        source_exceptions: Vec::new(),
        layers: Vec::new(),
        verification_classes: Vec::new(),
        forbidden_dependencies: Vec::new(),
        controlled_source_roots: Vec::new(),
        packages: vec![PackagePolicy {
            name: "fixture".to_owned(),
            path: PathBuf::from("fixture"),
            owner: "A0".to_owned(),
            layer: "engineering".to_owned(),
            verification_class: "C".to_owned(),
        }],
    }
}

fn write(fixture: &TestDirectory, relative: &str, contents: &str) -> PathBuf {
    let path = fixture.path().join(relative);
    fs::create_dir_all(path.parent().expect("fixture source must have a parent"))
        .expect("fixture source directory must be created");
    fs::write(&path, contents).expect("fixture source must be written");
    path
}

#[test]
fn rejects_aliased_include_with_a_non_rust_trusted_payload() {
    let fixture = TestDirectory::new();
    write(
        &fixture,
        "fixture/src/lib.rs",
        "use std::include as imported_source;\n\
         imported_source!(\"../../detached/payload.txt\");\n",
    );
    write(&fixture, "detached/payload.txt", "proof fn hidden() { admit(); }\n");

    let error = check(fixture.path(), &policy()).expect_err("include aliases must fail closed");
    assert_eq!(error.code(), ErrorCode::Trust);
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path() == Some(Path::new("fixture/src/lib.rs"))
            && diagnostic.message().contains("imports or re-exports `include`")
            && diagnostic.help().contains("built-in `include!`")
    }));
}

#[test]
fn rejects_include_import_variants() {
    let fixture = TestDirectory::new();
    write(
        &fixture,
        "fixture/src/lib.rs",
        r"
pub use core::include as public_source;
use std::{include as nested_source, fmt};
pub(crate) use core::{
    r#include as r#imported_source,
    str,
};
use standard_library::include;
",
    );

    let error = check(fixture.path(), &policy()).expect_err("include imports must fail closed");
    assert_eq!(
        error
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.message().contains("imports or re-exports `include`"))
            .count(),
        4
    );
}

#[test]
fn rejects_macro_forwarding_of_include() {
    let fixture = TestDirectory::new();
    write(
        &fixture,
        "fixture/src/lib.rs",
        r#"
macro_rules! load {
    ($m:ident, $p:literal) => { $m!($p); };
}
load!(include, "../../detached/payload.txt");
"#,
    );
    write(&fixture, "detached/payload.txt", "proof fn hidden() { admit(); }\n");

    let error = check(fixture.path(), &policy()).expect_err("macro forwarding must fail closed");
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.message().contains("defines `macro_rules!`")
            && diagnostic.help().contains("explicit Rust items")
    }));
    assert!(
        error.diagnostics().iter().any(|diagnostic| {
            diagnostic.message().contains("reserved code identifier `include`")
        })
    );
}

#[test]
fn rejects_macro_generated_path_attributes() {
    let fixture = TestDirectory::new();
    write(
        &fixture,
        "fixture/src/lib.rs",
        r#"
macro_rules! load_module {
    ($attr:ident, $file:literal) => {
        #[$attr = $file]
        mod hidden;
    };
}
load_module!(path, "../../detached/payload.txt");
"#,
    );
    write(&fixture, "detached/payload.txt", "proof fn hidden() { admit(); }\n");

    let error = check(fixture.path(), &policy())
        .expect_err("macro-generated path attributes must fail closed");
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.message().contains("defines `macro_rules!`"))
    );
}

#[test]
fn rejects_harmless_code_identifiers_named_include() {
    let fixture = TestDirectory::new();
    write(&fixture, "fixture/src/lib.rs", "fn include() { let _ = (); }\n");

    let error = check(fixture.path(), &policy()).expect_err("include is a reserved source token");
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.message().contains("reserved code identifier `include`")
            && diagnostic.help().contains("rename")
    }));
}

#[test]
fn include_words_in_non_code_tokens_are_allowed() {
    let fixture = TestDirectory::new();
    write(
        &fixture,
        "fixture/src/lib.rs",
        r#"
// use std::include as commented_out;
const EXAMPLE: &str = "pub use core::include as text";
const LETTER: char = 'i';
fn lifetime<'include>(value: &'include str) -> &'include str { value }
fn raw_keyword() { let r#use = EXAMPLE; let _ = r#use; }
"#,
    );

    let scanned = check(fixture.path(), &policy()).expect("non-code include words must be allowed");
    assert_eq!(scanned, 1);
}
