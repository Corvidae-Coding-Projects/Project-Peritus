//! Deterministic schema and compatibility-fixture generation tests.

use peritus_app_protocol::{
    AppProtocolLimits, decode_app_message,
    schema::{generated_fixture_cases, generated_text_artifacts, run_codegen},
};
use peritus_test_support::{CompatibilityCoverage, CompatibilityPolicy, FixtureCatalog};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[test]
fn checked_in_schema_and_fixture_assets_match_rust_metadata() {
    let temporary = tempfile::tempdir().expect("temporary generation root");
    generate(temporary.path(), false);

    for artifact in generated_text_artifacts() {
        let actual = std::fs::read_to_string(temporary.path().join(artifact.path))
            .expect("generated text artifact exists");
        assert_eq!(actual, artifact.content, "{} drifted", artifact.path);
    }

    let expected = generated_fixture_cases().expect("fixture source values encode");
    let catalog = FixtureCatalog::load(temporary.path().join("compat"))
        .expect("generated A2 compatibility catalog loads");
    assert_eq!(catalog.cases().len(), expected.len());
    assert_eq!(
        catalog
            .verify_compatibility_coverage(CompatibilityPolicy::RequireFixtures)
            .expect("all four fixture classes are present"),
        CompatibilityCoverage::Covered,
    );

    for fixture in expected {
        let observed = decode_app_message(&fixture.payload, AppProtocolLimits::PRODUCTION);
        if fixture.accepted {
            let message = observed.expect("valid generated fixture decodes");
            assert_eq!(Some(message.family()), fixture.expected_family, "{}", fixture.case);
        } else {
            let error = observed.expect_err("invalid generated fixture rejects");
            assert_eq!(Some(error.code()), fixture.expected_error, "{}", fixture.case);
        }
    }

    let root = workspace_root();
    generate(&root, true);
    assert_eq!(checked_in_case_names(&root), generated_case_names());
}

fn generate(root: &Path, check: bool) {
    let mut arguments = vec![OsString::from("--root"), root.as_os_str().to_owned()];
    if check {
        arguments.push(OsString::from("--check"));
    }
    run_codegen(arguments).expect("deterministic A3 code generation succeeds");
}

fn workspace_root() -> PathBuf {
    let mut current = std::env::current_dir().expect("test working directory");
    loop {
        if current.join("architecture.toml").is_file() && current.join("Cargo.toml").is_file() {
            return current;
        }
        assert!(current.pop(), "workspace root is an ancestor of the test process");
    }
}

fn generated_case_names() -> Vec<String> {
    let mut names = generated_fixture_cases()
        .expect("fixture source values encode")
        .into_iter()
        .map(|fixture| fixture.case.to_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn checked_in_case_names(root: &Path) -> Vec<String> {
    let mut names = std::fs::read_dir(root.join("compat/app-protocol/v1"))
        .expect("checked-in fixture root exists")
        .map(|entry| {
            entry
                .expect("fixture case entry")
                .file_name()
                .into_string()
                .expect("fixture case name is UTF-8")
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}
