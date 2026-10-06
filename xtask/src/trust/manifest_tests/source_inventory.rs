use super::*;
use crate::error::Diagnostic;
use crate::trust::manifest::validate_local;

const ORIGINAL: &str = "crates/foundation/peritus-types/src/lib.rs";

#[test]
fn removed_source_reports_stale_and_new_inputs_without_hiding_other_drift() {
    let fixture = Fixture::new();
    write_fixture(&fixture, "[]");
    fs::remove_file(fixture.path().join(ORIGINAL)).expect("old source must be removed");
    let replacement = "crates/foundation/peritus-types/src/value.rs";
    fixture.write(replacement, "pub fn value() -> u64 { 1 }\n");
    fixture.write(
        "crates/foundation/peritus-tcb/src/lib.rs",
        "fn audited() { assume(false); }\n#[test]\nfn evidence_case() { assert_eq!(1 + 1, 2); }\n",
    );
    let mut compilation = sources(&fixture);
    compilation[1] = fixture.path().join(replacement);
    let mut diagnostics = Vec::new();
    validate(
        fixture.path(),
        &policy(),
        &cargo(&fixture),
        &compilation,
        &[],
        false,
        &mut diagnostics,
    )
    .expect("removed inventory input must produce diagnostics instead of a read error");
    assert_diagnostic(&diagnostics, ORIGINAL, "stale or outside");
    assert_diagnostic(&diagnostics, replacement, "no proof-impact fingerprint");
    assert_diagnostic(
        &diagnostics,
        "crates/foundation/peritus-tcb/src/lib.rs",
        "differ from the reviewed",
    );
}

#[test]
fn declarations_outside_current_inventory_are_rejected_without_reading_them() {
    for stale in
        ["missing.rs", "crates/foundation/peritus-types/src", "../outside.rs", "/outside.rs"]
    {
        let fixture = Fixture::new();
        write_fixture(&fixture, "[]");
        replace_inventory_path(&fixture, stale);
        let mut diagnostics = Vec::new();
        validate_fixture(&fixture, &mut diagnostics)
            .expect("out-of-scope inventory declaration must aggregate diagnostics");
        assert_diagnostic(&diagnostics, stale, "stale or outside");
        assert_diagnostic(&diagnostics, ORIGINAL, "no proof-impact fingerprint");
    }
}

#[test]
fn local_validation_checks_current_trust_without_requiring_historical_approval() {
    let fixture = Fixture::new();
    write_fixture(&fixture, trust_entry());
    replace_inventory_path(&fixture, "removed.rs");
    let mut diagnostics = Vec::new();
    validate_local(
        fixture.path(),
        &policy(),
        &cargo(&fixture),
        &sources(&fixture),
        &[occurrence()],
        &mut diagnostics,
    )
    .expect("local trust validation must remain available with stale approval inventory");
    assert!(diagnostics.is_empty(), "unexpected local diagnostics: {diagnostics:?}");

    validate(
        fixture.path(),
        &policy(),
        &cargo(&fixture),
        &sources(&fixture),
        &[occurrence()],
        false,
        &mut diagnostics,
    )
    .expect("approval validation must aggregate stale inventory diagnostics");
    assert_diagnostic(&diagnostics, "removed.rs", "stale or outside");

    diagnostics.clear();
    validate_local(
        fixture.path(),
        &policy(),
        &cargo(&fixture),
        &sources(&fixture),
        &[],
        &mut diagnostics,
    )
    .expect("local validation must still reconcile current trust entries");
    assert!(diagnostics.iter().any(|item| item.message().contains("does not match exactly one")));
}

fn replace_inventory_path(fixture: &Fixture, stale: &str) {
    let manifest = "verification/proof-impact.toml";
    let contents = fs::read_to_string(fixture.path().join(manifest))
        .expect("proof-impact fixture must be readable");
    fixture.write(manifest, &contents.replace(ORIGINAL, stale));
}

fn assert_diagnostic(diagnostics: &[Diagnostic], path: &str, message: &str) {
    assert!(
        diagnostics.iter().any(|item| {
            item.path() == Some(Path::new(path)) && item.message().contains(message)
        }),
        "missing `{message}` diagnostic for `{path}`: {diagnostics:?}"
    );
}
