use super::reproducibility_workspace::validate_contents;
use crate::error::Diagnostic;
use std::path::Path;

const CANONICAL: &str = include_str!("../../../Cargo.toml");

#[test]
fn canonical_workspace_contract_is_accepted() {
    assert!(validate(CANONICAL).is_empty());
}

#[test]
fn lint_deletion_weakening_and_unreviewed_cfg_are_rejected() {
    for altered in [
        changed("unsafe_code = \"forbid\"\n", ""),
        changed("missing_docs = \"deny\"", "missing_docs = \"warn\""),
        changed(
            "    \"cfg(verus_verify_core)\",",
            "    \"cfg(verus_verify_core)\",\n    \"cfg(attacker_controlled)\",",
        ),
    ] {
        assert_message(&validate(&altered), "lint policy");
    }
}

#[test]
fn resolver_metadata_and_profiles_cannot_drift() {
    for (altered, expected) in [
        (changed("resolver = \"3\"", "resolver = \"2\""), "workspace resolver"),
        (changed("architecture-policy = \"architecture.toml\"\n", ""), "Peritus policy metadata"),
        (changed("overflow-checks = true", "overflow-checks = false"), "development profile"),
        (
            changed(
                "[profile.test]\nincremental = false\noverflow-checks = true",
                "[profile.test]\nincremental = true\noverflow-checks = true",
            ),
            "test profile",
        ),
        (changed("panic = \"abort\"", "panic = \"unwind\""), "release profile"),
        (changed("edition = \"2024\"", "edition = \"2021\""), "package metadata"),
    ] {
        assert_message(&validate(&altered), expected);
    }
}

fn changed(needle: &str, replacement: &str) -> String {
    let altered = CANONICAL.replacen(needle, replacement, 1);
    assert_ne!(altered, CANONICAL, "fixture mutation `{needle}` must apply");
    altered
}

fn validate(contents: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    validate_contents(contents, Path::new("Cargo.toml"), &mut diagnostics)
        .expect("fixture manifest must parse");
    diagnostics
}

fn assert_message(diagnostics: &[Diagnostic], expected: &str) {
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message().contains(expected)),
        "expected `{expected}`, got {diagnostics:?}"
    );
}
