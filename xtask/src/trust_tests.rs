use super::check_fixture as check;
use super::construct::Construct;
use super::lexer::scan;
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
        let path = env::temp_dir().join(format!("peritus-xtask-trust-{}-{id}", process::id()));
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

fn policy(trusted_source_roots: Vec<PathBuf>) -> ArchitecturePolicy {
    ArchitecturePolicy {
        schema: 1,
        soft_source_lines: 400,
        hard_source_lines: 700,
        root_module_lines: 80,
        required_license: "MIT".to_owned(),
        ignored_directories: Vec::new(),
        forbidden_module_names: Vec::new(),
        trusted_source_roots,
        source_exceptions: Vec::new(),
        layers: Vec::new(),
        verification_classes: Vec::new(),
        forbidden_dependencies: Vec::new(),
        controlled_source_roots: Vec::new(),
        refinement_reservations: Vec::new(),
        packages: vec![PackagePolicy {
            name: "fixture".to_owned(),
            path: PathBuf::from("fixture"),
            owner: "A0".to_owned(),
            layer: "engineering".to_owned(),
            verification_class: "C".to_owned(),
        }],
    }
}

fn write_source(fixture: &TestDirectory, relative: &str, contents: &str) -> PathBuf {
    let path = fixture.path().join(relative);
    fs::create_dir_all(path.parent().expect("fixture source must have a parent"))
        .expect("fixture source directory must be created");
    fs::write(&path, contents).expect("fixture source must be written");
    path
}

#[test]
fn detects_every_required_escape_hatch() {
    let source = r#"
proof fn unsound() {
    assume
        /* whitespace cannot hide the call */
        (false);
    admit
        ();
}

pub axiom
fn invented_fact();

pub assume_specification
    [external_crate::operation]
    ();

#[verifier
    :: external_body]
fn trusted_body() { let _value = 1; }

#[cfg_attr(verus_keep_ghost, verifier
    :: external)]
fn ignored_item() { let _value = 2; }

#[verifier::external_fn_specification]
fn function_proxy() { let _value = 3; }
#[verifier::external_type_specification]
struct TypeProxy(ExternalType);
#[verifier::external_trait_specification]
trait TraitProxy {}

#[verifier::assume_termination]
fn termination_escape() { let _value = 4; }
#![verifier::exec_allows_no_decreases_clause]

exec_spec_unverified! {
    spec fn unchecked_translation() -> bool { true }
}

inline_air_stmt
    ("(assume false)");

let compiler_escape = "--allow-inline-air";
"#;

    let constructs: Vec<Construct> =
        scan(source).into_iter().map(|occurrence| occurrence.construct).collect();
    for required in [
        Construct::Assume,
        Construct::Admit,
        Construct::Axiom,
        Construct::AssumeSpecification,
        Construct::External,
        Construct::ExternalBody,
        Construct::ExternalFunctionSpecification,
        Construct::ExternalTypeSpecification,
        Construct::ExternalTraitSpecification,
        Construct::AssumeTermination,
        Construct::ExecAllowsNoDecreases,
        Construct::ExecSpecUnverified,
        Construct::InlineAirStatement,
        Construct::AllowInlineAir,
    ] {
        assert!(constructs.contains(&required), "missing {required:?}");
    }
}

#[test]
fn detects_every_pinned_external_attribute_spelling() {
    let source = r"
#[verifier::external]
fn path_external() { let _ = (); }
#[verifier(external)]
fn list_external() { let _ = (); }
#[verus_verify(external)]
fn macro_external() { let _ = (); }

#[verifier::external_body]
fn path_body() { let _ = (); }
#[verifier(external_body)]
fn list_body() { let _ = (); }
#[verus_verify(external_body)]
fn macro_body() { let _ = (); }

#[verus_verify(external_type_specification)]
struct TypeProxy(ExternalType);
#[verifier::external_trait_extension(Spec via Impl)]
trait Extended {}
#[verifier::external_trait_private_bound(core::private::Sealed)]
trait Sealed {}
#[verifier::external_derive]
struct Derived;
#[verus::internal(external_trait_blanket)]
impl<T> Extended for T {}
#[verus::trusted]
fn trusted() { let _ = (); }
";

    let constructs: Vec<Construct> =
        scan(source).into_iter().map(|occurrence| occurrence.construct).collect();
    assert_eq!(constructs.iter().filter(|item| **item == Construct::External).count(), 3);
    assert_eq!(constructs.iter().filter(|item| **item == Construct::ExternalBody).count(), 3);
    for required in [
        Construct::ExternalTypeSpecification,
        Construct::ExternalTraitExtension,
        Construct::ExternalTraitPrivateBound,
        Construct::ExternalDerive,
        Construct::ExternalTraitBlanket,
        Construct::Trusted,
    ] {
        assert!(constructs.contains(&required), "missing {required:?}");
    }
}

#[test]
fn external_words_outside_attributes_are_not_trust_markers() {
    let source = r#"
fn record_external(external: bool) -> bool { external }
let external_body = "ordinary identifier";
let external_trait_extension = false;
"#;
    assert!(scan(source).is_empty());
}

#[test]
fn ordinary_admit_method_declarations_and_calls_are_not_trust_markers() {
    let source = r"
impl EvidenceStore {
    pub fn admit(&mut self, draft: EvidenceDraft) -> Result<(), EvidenceError> { self.catalog.admit(draft) }
}
let record = store.admit(draft)?;
let admit = false;
";
    assert!(scan(source).is_empty());
}

#[test]
fn qualified_verus_admit_call_remains_a_trust_marker() {
    let occurrences = scan("vstd::pervasive::admit();");
    assert_eq!(occurrences.len(), 1);
    assert_eq!(occurrences[0].construct, Construct::Admit);
}

#[test]
fn ignores_comments_and_non_security_literal_contents() {
    let source = r####"
// assume(false); admit(); #[verifier::external]
/* outer external_body
   /* nested axiom fn hidden(); */
   assume_specification[hidden]();
*/
let ordinary = "assume(false); #[verifier::external_body]";
let raw = r###"admit(); external_trait_specification inline_air_stmt"###;
let byte = b"axiom assume_specification";
let raw_byte = br##"external_fn_specification"##;
let character = 'a';
let byte_character = b'x';
fn lifetime_name<'assume>(value: &'assume str) -> &'assume str { value }
let assumeé = true;
"####;

    assert!(scan(source).is_empty());
}

#[test]
fn detects_raw_identifiers_and_import_aliases() {
    let source = r"
use vstd::prelude::{
    r#assume
        as accept_without_proof,
    admit as finish_without_proof,
};

accept_without_proof(false);
finish_without_proof();
";

    let occurrences = scan(source);
    assert_eq!(
        occurrences
            .iter()
            .filter(|item| item.construct == Construct::ProhibitedTrustedImport)
            .count(),
        2
    );
}

#[test]
fn confirmed_whitespace_bypass_returns_actionable_typed_failure() {
    let fixture = TestDirectory::new();
    write_source(
        &fixture,
        "fixture/src/boundary.rs",
        "proof fn bypass() { assume /* split */\n(false); }\n",
    );

    let error = check(fixture.path(), &policy(Vec::new()))
        .expect_err("token-separated trusted construct must be rejected");
    assert_eq!(error.code(), ErrorCode::Trust);
    assert_eq!(error.diagnostics()[0].path(), Some(Path::new("fixture/src/boundary.rs")));
    assert!(error.diagnostics()[0].message().contains("line 1"));
    assert!(error.diagnostics()[0].help().contains("peritus-tcb"));
}

#[test]
fn unowned_repository_rust_is_trust_scanned() {
    let fixture = TestDirectory::new();
    write_source(&fixture, "detached/boundary.rs", "proof fn detached() { admit(); }\n");

    let error = check(fixture.path(), &policy(Vec::new()))
        .expect_err("unowned repository Rust must be scanned");
    assert_eq!(error.code(), ErrorCode::Trust);
    assert_eq!(error.diagnostics()[0].path(), Some(Path::new("detached/boundary.rs")));
}

#[test]
fn path_and_include_external_rust_are_trust_scanned() {
    let fixture = TestDirectory::new();
    write_source(
        &fixture,
        "fixture/src/lib.rs",
        r#"
#[path = "../../detached/path_boundary.rs"]
mod path_boundary;
include!("../../detached/include_boundary.rs");
"#,
    );
    write_source(
        &fixture,
        "detached/path_boundary.rs",
        "proof fn path_escape() { assume(false); }\n",
    );
    write_source(
        &fixture,
        "detached/include_boundary.rs",
        "proof fn include_escape() { admit(); }\n",
    );

    let error = check(fixture.path(), &policy(Vec::new()))
        .expect_err("external Rust sources must be scanned");
    let paths: Vec<&Path> =
        error.diagnostics().iter().filter_map(|diagnostic| diagnostic.path()).collect();
    assert!(paths.contains(&Path::new("detached/path_boundary.rs")));
    assert!(paths.contains(&Path::new("detached/include_boundary.rs")));
}

#[test]
fn ignored_provider_reference_and_build_directories_are_not_scanned() {
    let fixture = TestDirectory::new();
    for path in [
        "provider-cache/boundary.rs",
        "reference-repos/boundary.rs",
        "target/generated/boundary.rs",
    ] {
        write_source(&fixture, path, "proof fn ignored() { admit(); }\n");
    }
    let mut policy = policy(Vec::new());
    policy.ignored_directories =
        vec!["provider-cache".to_owned(), "reference-repos".to_owned(), "target".to_owned()];

    let scanned = check(fixture.path(), &policy).expect("ignored directories must not be scanned");
    assert_eq!(scanned, 0);
}

#[path = "trust/dependency_escape_tests.rs"]
mod dependency_escape_tests;

#[test]
fn ordinary_non_rust_files_are_not_trust_scanned() {
    let fixture = TestDirectory::new();
    write_source(&fixture, "notes/proof.md", "assume(false)\n");
    write_source(&fixture, "scripts/check.sh", "admit\n");

    let scanned = check(fixture.path(), &policy(Vec::new()))
        .expect("ordinary non-Rust files must not be scanned");
    assert_eq!(scanned, 0);
}

#[test]
fn trusted_roots_are_scanned_but_their_constructs_remain_allowed() {
    let fixture = TestDirectory::new();
    write_source(&fixture, "fixture/src/boundary.rs", "proof fn audited() { admit(); }\n");
    let policy = policy(vec![PathBuf::from("fixture")]);

    let scanned = check(fixture.path(), &policy).expect("trusted source should remain allowed");
    assert_eq!(scanned, 1);
}

#[cfg(unix)]
#[test]
fn symbolic_link_source_fails_closed_even_in_a_trusted_root() {
    use std::os::unix::fs::symlink;

    let fixture = TestDirectory::new();
    let target = write_source(&fixture, "outside.rs", "fn harmless() -> bool { true }\n");
    let link = fixture.path().join("fixture/src/boundary.rs");
    fs::create_dir_all(link.parent().expect("link must have a parent"))
        .expect("link directory must be created");
    symlink(target, &link).expect("fixture symbolic link must be created");

    let error = check(fixture.path(), &policy(vec![PathBuf::from("fixture")]))
        .expect_err("symbolic links must fail closed");
    assert_eq!(error.code(), ErrorCode::Trust);
    assert!(error.diagnostics()[0].message().contains("symbolic link"));
    assert!(error.diagnostics()[0].help().contains("cannot be redirected"));
}

#[cfg(unix)]
#[test]
fn unowned_crates_symbolic_link_source_fails_closed() {
    use std::os::unix::fs::symlink;

    let fixture = TestDirectory::new();
    let target = write_source(&fixture, "outside.txt", "fn harmless() -> bool { true }\n");
    write_source(
        &fixture,
        "fixture/src/lib.rs",
        "include!(\"../../crates/unowned/src/boundary\");\n",
    );
    let link = fixture.path().join("crates/unowned/src/boundary");
    fs::create_dir_all(link.parent().expect("link must have a parent"))
        .expect("link directory must be created");
    symlink(target, &link).expect("fixture symbolic link must be created");

    let error = check(fixture.path(), &policy(Vec::new()))
        .expect_err("unowned crates symbolic links must fail closed");
    assert_eq!(error.code(), ErrorCode::Trust);
    assert_eq!(error.diagnostics()[0].path(), Some(Path::new("crates/unowned/src/boundary")));
    assert!(error.diagnostics()[0].message().contains("symbolic link"));
}
