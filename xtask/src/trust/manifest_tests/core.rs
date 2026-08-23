use super::*;

#[test]
fn exact_entry_and_occurrence_reconcile_one_to_one() {
    let fixture = Fixture::new();
    write_fixture(&fixture, trust_entry());
    let mut diagnostics = Vec::new();
    validate(
        fixture.path(),
        &policy(),
        &cargo(&fixture),
        &sources(&fixture),
        &[occurrence()],
        false,
        &mut diagnostics,
    )
    .expect("valid manifests must parse");
    assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
}

#[test]
fn rejects_untracked_and_stale_trusted_occurrences() {
    let fixture = Fixture::new();
    write_fixture(&fixture, "[]");
    let mut diagnostics = Vec::new();
    validate(
        fixture.path(),
        &policy(),
        &cargo(&fixture),
        &sources(&fixture),
        &[occurrence()],
        false,
        &mut diagnostics,
    )
    .expect("empty manifests must parse");
    assert!(diagnostics.iter().any(|item| item.message().contains("no unique manifest entry")));

    write_fixture(&fixture, trust_entry());
    diagnostics.clear();
    validate(
        fixture.path(),
        &policy(),
        &cargo(&fixture),
        &sources(&fixture),
        &[],
        false,
        &mut diagnostics,
    )
    .expect("stale manifests must still parse");
    assert!(diagnostics.iter().any(|item| item.message().contains("does not match exactly one")));
}

#[test]
fn unknown_manifest_fields_fail_closed_during_parsing() {
    let fixture = Fixture::new();
    write_fixture(&fixture, "[]");
    fixture.write(
        "verification/trust.toml",
        "schema = 'peritus.verification.trust'\nschema_version = 1\nbaseline = 'A1'\nentries = []\nextra = true\n",
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
    .expect("schema failures must aggregate as trust diagnostics");
    assert!(diagnostics.iter().any(|item| item.message().contains("TOML schema")));
}

#[test]
fn exclusion_and_excluded_obligation_reconcile_by_owner_and_symbol() {
    let fixture = Fixture::new();
    write_fixture(&fixture, "[]");
    write_coverage_documents(&fixture, exclusion_entry(), excluded_obligation());
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
    .expect("valid coverage manifests must parse");
    assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
}

#[test]
fn exclusion_class_expiry_and_evidence_commands_fail_closed() {
    let fixture = Fixture::new();
    write_fixture(&fixture, "[]");
    let altered = exclusion_entry()
        .replace("verification_class = \"T\"", "verification_class = \"H\"")
        .replace("--all-features --locked", "--all-features")
        .replace("revisit_by = \"2099-08-20\"", "revisit_by = \"2026-08-20\"");
    write_coverage_documents(&fixture, &altered, "[]");
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
    .expect("structurally valid altered manifests must parse");
    for expected in
        ["disagrees with its registered package", "exact locked package gate", "expired"]
    {
        assert!(
            diagnostics.iter().any(|item| item.message().contains(expected)),
            "missing `{expected}` in {diagnostics:?}"
        );
    }
}

#[test]
fn obligation_status_cross_references_and_cycles_fail_closed() {
    let fixture = Fixture::new();
    write_fixture(&fixture, "[]");
    let first = excluded_obligation()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .replace("status = \"excluded\"", "status = \"open\"")
        .replace("dependencies = []", "dependencies = [\"OBL-0002\"]")
        .replace("exclusion_id = \"EXCL-0001\"", "reviewer = \"invented-reviewer\"");
    let second = first.replace("OBL-0001", "OBL-0002").replace("OBL-0002\"]", "OBL-0001\"]");
    write_coverage_documents(&fixture, "[]", &format!("[{first},{second}]"));
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
    .expect("structurally valid altered obligations must parse");
    assert!(diagnostics.iter().any(|item| item.message().contains("cannot claim review")));
    assert!(diagnostics.iter().any(|item| item.message().contains("contains a cycle")));
}

#[test]
fn upstream_versions_reject_floating_refs_in_composite_text() {
    for pinned in [
        "1.2.3",
        "commit 92f466f247f45128c630d1c843fd6e27d2115587",
        "ABI 4 through 6",
        "release 0.2026.08.09.92f466f",
    ] {
        assert!(version_is_pinned(pinned), "rejected pinned reference `{pinned}`");
    }
    for floating in [
        "latest version 1",
        "main at build 42",
        "1.*",
        "^1.2.3",
        "1.x",
        "nightly-2026-08-21",
        "no immutable revision",
        "unreviewed build 1",
        "release abcdefa",
        "ABI 6 through 4",
    ] {
        assert!(!version_is_pinned(floating), "accepted floating reference `{floating}`");
    }
}

#[test]
fn exclusion_symbols_distinguish_same_named_associated_functions() {
    let fixture = Fixture::new();
    let relative = "crates/state/peritus-approval/src/digest.rs";
    fixture.write(
        relative,
        r"
pub struct ApprovalRequestDigest;
pub struct ApprovalDecisionDigest;

impl ApprovalRequestDigest {
    pub fn compute() -> u8 { 1 }
}

impl ApprovalDecisionDigest {
    pub fn compute() -> u8 { 2 }
}
",
    );
    let source = fixture.path().join(relative);

    for symbol in [
        "peritus_approval::digest::ApprovalRequestDigest::compute",
        "peritus_approval::digest::ApprovalDecisionDigest::compute",
    ] {
        let mut diagnostics = Vec::new();
        validate_symbol(
            Path::new("verification/exclusions.toml"),
            "EXCL-0002",
            "peritus-approval",
            Some(&source),
            symbol,
            &mut diagnostics,
        );
        assert!(diagnostics.is_empty(), "rejected `{symbol}`: {diagnostics:?}");
    }

    let mut diagnostics = Vec::new();
    validate_symbol(
        Path::new("verification/exclusions.toml"),
        "EXCL-0002",
        "peritus-approval",
        Some(&source),
        "peritus_approval::digest::compute",
        &mut diagnostics,
    );
    assert!(
        diagnostics.iter().any(|item| item.message().contains("source module path")),
        "collapsed associated symbol was accepted: {diagnostics:?}"
    );
}

#[test]
fn exclusion_symbols_ignore_declaration_shaped_non_items() {
    let fixture = Fixture::new();
    let relative = "crates/state/peritus-approval/src/digest.rs";
    fixture.write(
        relative,
        r#"
// pub fn compute() -> u8 { 1 }
const TEXT: &str = "pub fn compute() -> u8 { 1 }";

pub fn enclosing() {
    fn compute() -> u8 { 1 }
    let _value = compute();
}
"#,
    );
    let source = fixture.path().join(relative);
    let mut diagnostics = Vec::new();
    validate_symbol(
        Path::new("verification/exclusions.toml"),
        "EXCL-0002",
        "peritus-approval",
        Some(&source),
        "peritus_approval::digest::compute",
        &mut diagnostics,
    );
    assert!(
        diagnostics.iter().any(|item| item.message().contains("source module path")),
        "non-item declaration text was accepted: {diagnostics:?}"
    );
}

#[test]
fn exclusion_symbols_include_inline_modules_and_trait_owners() {
    let fixture = Fixture::new();
    let relative = "crates/state/peritus-approval/src/digest.rs";
    fixture.write(
        relative,
        r"
mod version_one {
    pub trait CanonicalDigest {
        fn compute();
    }
}
",
    );
    let source = fixture.path().join(relative);
    let mut diagnostics = Vec::new();
    validate_symbol(
        Path::new("verification/exclusions.toml"),
        "EXCL-0002",
        "peritus-approval",
        Some(&source),
        "peritus_approval::digest::version_one::CanonicalDigest::compute",
        &mut diagnostics,
    );
    assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
}
