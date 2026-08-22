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
