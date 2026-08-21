use super::check_fixture as check;
use crate::error::ErrorCode;
use crate::model::{ArchitecturePolicy, ControlledSourceKind, ControlledSourceRoot, PackagePolicy};
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
        let path =
            env::temp_dir().join(format!("peritus-xtask-trust-source-{}-{id}", process::id()));
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
fn pinned_external_list_spellings_fail_end_to_end() {
    let fixture = TestDirectory::new();
    write(
        &fixture,
        "fixture/src/lib.rs",
        r"
#[verifier(external)]
fn verifier_list() { let _ = (); }
#[verus_verify(external)]
fn verify_macro_list() { let _ = (); }
",
    );

    let error = check(fixture.path(), &policy()).expect_err("external items must be rejected");
    assert_eq!(error.code(), ErrorCode::Trust);
    assert_eq!(
        error
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.message().contains("trusted construct `external`"))
            .count(),
        2
    );
}

#[test]
fn ignore_prefixes_are_anchored_at_the_repository_root() {
    let fixture = TestDirectory::new();
    write(&fixture, "target/ignored.rs", "proof fn ignored() { admit(); }\n");
    write(
        &fixture,
        "fixture/src/target/evil.rs",
        "proof fn nested_target_is_source() { admit(); }\n",
    );
    let mut policy = policy();
    policy.ignored_directories.push("target".to_owned());

    let error = check(fixture.path(), &policy).expect_err("nested target must not be ignored");
    assert_eq!(error.code(), ErrorCode::Trust);
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path() == Some(Path::new("fixture/src/target/evil.rs"))
            && diagnostic.message().contains("admit")
    }));
}

#[test]
fn include_and_path_scan_code_regardless_of_extension() {
    let fixture = TestDirectory::new();
    write(
        &fixture,
        "fixture/src/lib.rs",
        r#"
include!("../../detached/include.code");
#[path = "../../detached/path_module"]
mod path_module;
"#,
    );
    write(&fixture, "detached/include.code", "proof fn included() { assume(false); }\n");
    write(&fixture, "detached/path_module", "proof fn module() { admit(); }\n");

    let error = check(fixture.path(), &policy()).expect_err("code-bearing inputs must be scanned");
    let paths: Vec<&Path> =
        error.diagnostics().iter().filter_map(|diagnostic| diagnostic.path()).collect();
    assert!(paths.contains(&Path::new("detached/include.code")));
    assert!(paths.contains(&Path::new("detached/path_module")));
}

#[test]
fn recursively_scans_included_non_rust_sources() {
    let fixture = TestDirectory::new();
    write(&fixture, "fixture/src/lib.rs", "include!(\"../../detached/outer.inc\");\n");
    write(&fixture, "detached/outer.inc", "include!(\"inner\");\n");
    write(&fixture, "detached/inner", "proof fn nested() { admit(); }\n");

    let error = check(fixture.path(), &policy()).expect_err("nested include must be scanned");
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path() == Some(Path::new("detached/inner"))
            && diagnostic.message().contains("admit")
    }));
}

#[test]
fn rejects_dynamic_include_and_path_inputs() {
    let fixture = TestDirectory::new();
    write(
        &fixture,
        "fixture/src/lib.rs",
        r#"
include!(concat!(env!("OUT_DIR"), "/generated.rs"));
#[path = generated_source!()]
mod generated;
"#,
    );

    let error = check(fixture.path(), &policy()).expect_err("dynamic inputs must fail closed");
    assert_eq!(
        error
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.message().contains("dynamic"))
            .count(),
        2
    );
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic.help().contains("OUT_DIR")));
}

#[test]
fn rejects_sources_outside_or_below_ignored_prefixes() {
    let fixture = TestDirectory::new();
    write(
        &fixture,
        "fixture/src/lib.rs",
        r#"
include!("../../../outside.inc");
include!("../../target/generated.inc");
"#,
    );
    write(&fixture, "target/generated.inc", "fn generated() { let _ = (); }\n");
    let mut policy = policy();
    policy.ignored_directories.push("target".to_owned());

    let error = check(fixture.path(), &policy).expect_err("unreviewable inputs must fail closed");
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.message().contains("outside the repository") })
    );
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.message().contains("ignored repository prefix") })
    );
}

#[test]
fn generated_sources_require_and_accept_narrow_controlled_ownership() {
    let fixture = TestDirectory::new();
    write(&fixture, "fixture/src/lib.rs", "include!(\"../../generated/source.inc\");\n");
    write(&fixture, "generated/source.inc", "fn checked_in() { let _ = (); }\n");

    let error = check(fixture.path(), &policy()).expect_err("unowned generated input must fail");
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path() == Some(Path::new("generated/source.inc"))
            && diagnostic.message().contains("ownership root")
    }));

    let mut owned = policy();
    owned.controlled_source_roots.push(ControlledSourceRoot {
        path: PathBuf::from("generated"),
        owner: "A0".to_owned(),
        kind: ControlledSourceKind::Generated,
        rationale: "A0 reviews checked-in generated Rust inputs.".to_owned(),
    });
    let scanned = check(fixture.path(), &owned).expect("controlled source must be scanned");
    assert_eq!(scanned, 2);
}

#[test]
fn current_xtask_style_path_attributes_are_supported() {
    let fixture = TestDirectory::new();
    write(
        &fixture,
        "fixture/src/lib.rs",
        r#"
#[path = "trust_lexer.rs"]
mod lexer;
#[cfg(test)]
#[path = "trust_tests.rs"]
mod tests;
"#,
    );
    write(&fixture, "fixture/src/trust_lexer.rs", "pub fn scan() { let _ = (); }\n");
    write(&fixture, "fixture/src/trust_tests.rs", "pub fn exercise() { let _ = (); }\n");

    let scanned = check(fixture.path(), &policy()).expect("literal sibling paths must be valid");
    assert_eq!(scanned, 3);
}

#[cfg(unix)]
#[test]
fn rejects_unignored_directory_symlinks_even_when_unreferenced() {
    use std::os::unix::fs::symlink;

    let fixture = TestDirectory::new();
    fs::create_dir_all(fixture.path().join("real")).expect("target directory must be created");
    symlink(fixture.path().join("real"), fixture.path().join("linked"))
        .expect("directory symbolic link must be created");

    let error = check(fixture.path(), &policy()).expect_err("directory symlink must fail closed");
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path() == Some(Path::new("linked"))
            && diagnostic.message().contains("directory is a symbolic link")
    }));
}

#[cfg(unix)]
#[test]
fn rejects_include_inputs_crossing_a_directory_symlink() {
    use std::os::unix::fs::symlink;

    let fixture = TestDirectory::new();
    write(&fixture, "fixture/src/lib.rs", "include!(\"../../linked/source.inc\");\n");
    write(&fixture, "real/source.inc", "proof fn redirected() { admit(); }\n");
    symlink(fixture.path().join("real"), fixture.path().join("linked"))
        .expect("directory symbolic link must be created");

    let error = check(fixture.path(), &policy()).expect_err("symlink component must fail closed");
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.message().contains("symbolic link component") })
    );
}
