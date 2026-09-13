use super::*;

#[test]
fn unconditional_executable_inside_verus_is_valid_proof_evidence() {
    let fixture = Fixture::new();
    write_fixture(&fixture, "[]");
    fixture.write(
        "crates/foundation/peritus-types/src/lib.rs",
        r"
verus! {
pub struct Capability;
impl Capability {
    pub const fn new() -> (value: u64)
        ensures value == 1,
    {
        1
    }
}
}
",
    );
    write_coverage_documents(&fixture, "[]", executable_obligation());
    let mut diagnostics = Vec::new();
    validate(
        fixture.path(),
        &policy(),
        &cargo(&fixture),
        &sources(&fixture),
        &[],
        false,
        &mut diagnostics,
    )
    .expect("executable proof-evidence fixture must parse");
    assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
}

#[test]
fn same_line_associated_declarations_keep_their_own_verus_mode() {
    let fixture = Fixture::new();
    write_fixture(&fixture, "[]");
    fixture.write(
        "crates/foundation/peritus-types/src/lib.rs",
        "verus! { pub struct Model; pub struct Runtime; impl Model { pub closed spec fn checked() -> bool { true } } impl Runtime { pub fn checked() -> (value: bool) ensures value, { true } } }\n",
    );
    let obligation =
        executable_obligation().replace("Capability::new", "Runtime::checked").replace(
            "the verified executable retains its exact result",
            "the exact associated executable retains its result",
        );
    write_coverage_documents(&fixture, "[]", &obligation);
    let mut diagnostics = Vec::new();
    validate(
        fixture.path(),
        &policy(),
        &cargo(&fixture),
        &sources(&fixture),
        &[],
        false,
        &mut diagnostics,
    )
    .expect("same-line associated evidence fixture must parse");
    assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");

    let registered = crate::trust::registered_proof_symbols(fixture.path())
        .expect("exact registered executable");
    assert_eq!(registered.len(), 1);
    assert_eq!(registered[0].mode, crate::api_contract::Mode::Exec);
}

#[test]
fn verus_evidence_rejects_ordinary_disabled_local_and_ambiguous_functions() {
    for source in [
        "pub fn checked() {}\n",
        "pub const fn checked() {}\n",
        "verus! { #[cfg(any())] pub fn checked() {} }\n",
        "verus! { fn outer() { pub fn checked() {} } }\n",
        "verus! { pub fn checked() {} pub fn checked() {} }\n",
    ] {
        let fixture = Fixture::new();
        write_fixture(&fixture, "[]");
        fixture.write("crates/foundation/peritus-types/src/lib.rs", source);
        write_coverage_documents(
            &fixture,
            "[]",
            &executable_obligation().replace("Capability::new", "checked"),
        );
        let mut diagnostics = Vec::new();
        validate(
            fixture.path(),
            &policy(),
            &cargo(&fixture),
            &sources(&fixture),
            &[],
            false,
            &mut diagnostics,
        )
        .expect("adversarial executable evidence fixture must parse");
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.message().contains("not exercised")
                    || diagnostic.message().contains("does not match exactly one")
            }),
            "ineligible executable evidence was accepted: {diagnostics:?}"
        );
    }
}

fn executable_obligation() -> &'static str {
    r##"[{
id = "OBL-0002",
kind = "contract",
statement = "the verified executable retains its exact result",
owning_crate = "peritus-types",
source_file = "crates/foundation/peritus-types/src/lib.rs",
symbol = "peritus_types::Capability::new",
status = "in-progress",
dependencies = [],
live_issue = "#3",
owner = "ACTOR-0001",
evidence = [{ kind = "verus-proof", source_file = "crates/foundation/peritus-types/src/lib.rs", symbol = "peritus_types::Capability::new", command = "cargo verus verify --package peritus-types --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20" }]
}]"##
}
