use super::check_with_roots;
use crate::model::ArchitecturePolicy;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("peritus-api-contract-{}-{sequence}", std::process::id()));
        fs::create_dir_all(root.join("fixture/src")).expect("fixture directory must be created");
        Self { root }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("fixture parent must be created");
        }
        fs::write(path, contents).expect("fixture source must be written");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn audits_non_rs_sources_reached_by_static_compilation_references() {
    let fixture = Fixture::new();
    fixture.write("fixture/src/lib.rs", "include!(\"boundary.inc\");\n");
    fixture.write(
        "fixture/src/boundary.inc",
        "#[verus_spec(requires value > 0)]\npub fn bypass(value: u64) { let _ = value; }\n",
    );
    let result = check(&fixture);

    let error = result.expect_err("included source precondition must fail the audit");
    assert!(error.to_string().contains("boundary.inc"));
    assert!(error.to_string().contains("requires"));
}

#[test]
fn rejects_shadowed_allowlisted_macros_before_they_can_generate_items() {
    let fixture = Fixture::new();
    fixture.write(
        "fixture/src/lib.rs",
        r"
macro_rules! assert {
    ($visibility:vis) => {
        #[verus_spec(requires false)]
        $visibility fn hidden() { let _ = 1_u8; }
    };
}
assert!(pub);
",
    );
    let result = check(&fixture);

    let error = result.expect_err("local expansion definitions must fail the audit");
    assert!(error.to_string().contains("macro_rules"));
}

#[test]
fn rejects_state_machine_invocation_without_an_import() {
    let fixture = Fixture::new();
    fixture.write("fixture/src/lib.rs", "state_machine! { Example { fields { value: nat } } }\n");

    let error = check(&fixture).expect_err("an unbound expansion name must fail closed");
    assert!(error.to_string().contains("state_machine"));
}

#[test]
fn rejects_even_an_unused_pinned_state_machine_import() {
    let fixture = Fixture::new();
    fixture.write("fixture/src/lib.rs", "use verus_state_machines_macros::state_machine;\n");

    let error = check(&fixture).expect_err("an imported proof-generating macro must fail closed");
    assert!(error.to_string().contains("macro"));
}

#[test]
fn rejects_all_ordinary_and_tokenized_state_machine_surfaces() {
    for (label, source) in [
        (
            "exact pinned ordinary macro",
            "use verus_state_machines_macros::state_machine;\nstate_machine! {}\n",
        ),
        ("alias", "use verus_state_machines_macros::state_machine as sm;\nsm! {}\n"),
        (
            "qualified",
            "use verus_state_machines_macros::state_machine;\nverus_state_machines_macros::state_machine! {}\n",
        ),
        ("glob", "use verus_state_machines_macros::*;\nstate_machine! {}\n"),
        (
            "tokenized",
            "use verus_state_machines_macros::tokenized_state_machine;\ntokenized_state_machine! {}\n",
        ),
        ("wrong crate", "use adversary::state_machine;\nstate_machine! {}\n"),
    ] {
        let fixture = Fixture::new();
        fixture.write("fixture/src/lib.rs", source);
        let error = check(&fixture).expect_err(&format!("{label} must fail closed"));
        assert!(error.to_string().contains("macro"), "{label}: {error}");
    }
}

#[test]
fn rejects_signing_surfaces_in_every_b1_production_root() {
    for package in [
        "peritus-approval",
        "peritus-budget",
        "peritus-leases",
        "peritus-policy",
        "renamed-b1-authority",
    ] {
        let fixture = Fixture::new();
        let relative = format!("{package}/src/lib.rs");
        fixture.write(&relative, "use ed25519_dalek::SigningKey as VerificationOnly;\n");
        let policy = b1_fixture_policy(package);
        let error =
            check_with_roots(Path::new(&fixture.root), &policy, &[fixture.root.join(&relative)])
                .expect_err("B1 signing APIs must fail closed");
        assert!(error.to_string().contains("SigningKey"), "{package}: {error}");
        assert!(error.to_string().contains("verifier-only"), "{package}: {error}");
    }
}

#[test]
fn verifier_only_rule_does_not_mistake_public_key_verification_for_signing() {
    let fixture = Fixture::new();
    fixture.write(
        "peritus-approval/src/lib.rs",
        "use ed25519_dalek::{Signature, VerifyingKey};\n\
         pub fn verify(key: &VerifyingKey, message: &[u8], signature: &Signature) -> bool {\n\
             key.verify_strict(message, signature).is_ok()\n\
         }\n",
    );
    check_with_roots(
        Path::new(&fixture.root),
        &b1_fixture_policy("peritus-approval"),
        &[fixture.root.join("peritus-approval/src/lib.rs")],
    )
    .expect("strict verification must remain available");
}

fn check(fixture: &Fixture) -> Result<super::Report, crate::error::XtaskError> {
    check_with_roots(
        Path::new(&fixture.root),
        &fixture_policy(),
        &[fixture.root.join("fixture/src/lib.rs")],
    )
}

fn fixture_policy() -> ArchitecturePolicy {
    toml::from_str(
        r#"
schema = 2
soft_source_lines = 400
hard_source_lines = 700
root_module_lines = 80
required_license = "MIT"
ignored_directories = ["target"]
forbidden_module_names = []
trusted_source_roots = []
source_exceptions = []
layers = []
verification_classes = []
forbidden_dependencies = []
controlled_source_roots = []

[[packages]]
name = "fixture"
path = "fixture"
owner = "A1"
layer = "foundation"
verification_class = "V"
"#,
    )
    .expect("fixture policy must parse")
}

fn b1_fixture_policy(package: &str) -> ArchitecturePolicy {
    toml::from_str(&format!(
        r#"
schema = 2
soft_source_lines = 400
hard_source_lines = 700
root_module_lines = 80
required_license = "MIT"
ignored_directories = ["target"]
forbidden_module_names = []
trusted_source_roots = []
source_exceptions = []
layers = []
verification_classes = []
forbidden_dependencies = []
controlled_source_roots = []

[[packages]]
name = "{package}"
path = "{package}"
owner = "B1"
layer = "state"
verification_class = "H"
"#,
    ))
    .expect("B1 fixture policy must parse")
}
