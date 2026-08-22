use super::*;

#[test]
fn evidence_must_be_an_unconditionally_exercised_test() {
    for source in [
        "fn audited() { assume(false); }\nfn evidence_case() { let _value = 1; }\n",
        "fn audited() { assume(false); }\n#[cfg(any())]\n#[test]\nfn evidence_case() { let _value = 1; }\n",
    ] {
        let fixture = Fixture::new();
        write_fixture(&fixture, trust_entry());
        fixture.write("crates/foundation/peritus-tcb/src/lib.rs", source);
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
        .expect("altered evidence fixture must parse");
        assert!(
            diagnostics.iter().any(|item| item.message().contains("not exercised")),
            "missing executable-evidence diagnostic: {diagnostics:?}"
        );
    }
}

#[test]
fn trusted_symbol_must_match_the_exact_module_path() {
    let fixture = Fixture::new();
    write_fixture(
        &fixture,
        &trust_entry().replace("peritus_tcb::audited", "peritus_tcb::nested::audited"),
    );
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
    .expect("altered symbol fixture must parse");
    assert!(diagnostics.iter().any(|item| item.message().contains("source module path")));
    assert!(diagnostics.iter().any(|item| item.message().contains("does not match exactly one")));
}

#[test]
fn unknown_proof_impact_fields_fail_closed_during_parsing() {
    let fixture = Fixture::new();
    write_fixture(&fixture, "[]");
    let path = fixture.path().join("verification/proof-impact.toml");
    let contents = fs::read_to_string(&path).expect("fixture manifest must be readable");
    fixture.write("verification/proof-impact.toml", &format!("{contents}\nextra = true\n"));
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
fn formal_package_manifest_drift_requires_proof_impact_review() {
    let fixture = Fixture::new();
    write_fixture(&fixture, "[]");
    fixture.write(
        "crates/foundation/peritus-tcb/Cargo.toml",
        "[package]\nname='peritus-tcb'\n[features]\nunreviewed=[]\n",
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
    .expect("manifest-only drift fixture must parse");
    assert!(diagnostics.iter().any(|item| {
        item.path() == Some(Path::new("crates/foundation/peritus-tcb/Cargo.toml"))
            && item.message().contains("differ")
    }));
}

#[test]
fn every_shared_semantics_input_requires_exact_reviewed_bytes() {
    for path in [
        ".cargo/config.toml",
        "Cargo.lock",
        "Cargo.toml",
        "architecture.toml",
        "rust-toolchain.toml",
        "toolchains.toml",
        "verification/actor-provenance.json",
        "verification/actors.toml",
        "verification/exclusions.toml",
        "verification/obligations.toml",
        "verification/trust.toml",
    ] {
        let fixture = Fixture::new();
        write_fixture(&fixture, "[]");
        let original = fs::read_to_string(fixture.path().join(path))
            .expect("shared fixture input must be readable");
        let altered = if Path::new(path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        {
            format!(" {original}")
        } else {
            format!("{original}\n# unreviewed semantic rewrite\n")
        };
        fixture.write(path, &altered);
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
        .expect("shared-input drift fixture must parse");
        assert!(
            diagnostics.iter().any(|item| {
                item.path() == Some(Path::new(path)) && item.message().contains("differ")
            }),
            "missing proof-impact drift diagnostic for `{path}`: {diagnostics:?}"
        );
    }
}

#[test]
fn actor_references_require_registered_role_and_independence() {
    let fixture = Fixture::new();
    write_fixture(&fixture, "[]");
    let path = fixture.path().join("verification/proof-impact.toml");
    let contents = fs::read_to_string(&path).expect("fixture manifest must be readable");

    for (reviewer, expected) in
        [("ACTOR-9999", "unregistered actor"), ("ACTOR-0001", "without `reviewer` role")]
    {
        fixture.write("verification/proof-impact.toml", &contents.replace("ACTOR-0002", reviewer));
        let mut diagnostics = Vec::new();
        validate_fixture(&fixture, &mut diagnostics).expect("altered actor reference must parse");
        assert!(
            diagnostics.iter().any(|item| item.message().contains(expected)),
            "missing `{expected}` in {diagnostics:?}"
        );
    }
}

#[test]
fn duplicate_actor_identity_and_placeholder_provenance_fail_closed() {
    let fixture = Fixture::new();
    write_fixture(&fixture, "[]");
    let path = fixture.path().join("verification/actors.toml");
    let contents = fs::read_to_string(&path).expect("fixture actor registry must be readable");
    let duplicate =
        contents.replace("[[entries]]\nid = \"ACTOR-0002\"", "[[entries]]\nid = \"ACTOR-0001\"");
    fixture.write("verification/actors.toml", &duplicate);
    let path = fixture.path().join("verification/actor-provenance.json");
    let provenance = fs::read_to_string(&path).expect("fixture provenance must be readable");
    fixture.write(
        "verification/actor-provenance.json",
        &provenance.replace(
            "\"issue_created_at\": \"2026-08-21T21:10:43.329647070Z\"",
            "\"issue_created_at\": \"tbd\"",
        ),
    );
    let mut diagnostics = Vec::new();
    validate_fixture(&fixture, &mut diagnostics).expect("altered actor registry must parse");
    assert!(diagnostics.iter().any(|item| item.message().contains("placeholder text")));
    assert!(diagnostics.iter().any(|item| item.message().contains("declared more than once")));
}

#[test]
fn actor_aliases_cannot_claim_independent_roles_with_different_provenance() {
    let fixture = Fixture::new();
    write_fixture(&fixture, "[]");
    let path = fixture.path().join("verification/actors.toml");
    let contents = fs::read_to_string(&path).expect("fixture actor registry must be readable");
    let aliased = contents
        .replace("kind = \"codex-subagent\"", "kind = \"crosslink-agent\"")
        .replace(
            "principal = \"Corvidae-Coding-Projects/Project-Peritus/session/2/task/root/fixture_reviewer\"",
            "principal = \"SHA256:eV8eZPaZxut5mrkihmvsOTrGWClwD+B/HR//do+oIeI\"",
        )
        ;
    fixture.write("verification/actors.toml", &aliased);
    let path = fixture.path().join("verification/actor-provenance.json");
    let provenance = fs::read_to_string(&path).expect("fixture provenance must be readable");
    let aliased_provenance = provenance
        .replace("\"kind\": \"codex-subagent\"", "\"kind\": \"crosslink-agent\"")
        .replace(
            "\"principal\": \"Corvidae-Coding-Projects/Project-Peritus/session/2/task/root/fixture_reviewer\"",
            "\"principal\": \"SHA256:eV8eZPaZxut5mrkihmvsOTrGWClwD+B/HR//do+oIeI\"",
        )
        .replace(
            "\"session\": 2,\n      \"task\": \"/root/fixture_reviewer\"",
            "\"session\": 9,\n      \"task\": \"/root/fixture_reviewer\"",
        );
    fixture.write("verification/actor-provenance.json", &aliased_provenance);
    let mut diagnostics = Vec::new();
    validate_fixture(&fixture, &mut diagnostics).expect("aliased actor registry must parse");
    for expected in [
        "aliases an already registered external subject",
        "aliases one actor as both owner and reviewer",
    ] {
        assert!(
            diagnostics.iter().any(|item| item.message().contains(expected)),
            "missing `{expected}` in {diagnostics:?}"
        );
    }
}

#[test]
fn actor_task_must_be_root_or_a_root_descendant() {
    let fixture = Fixture::new();
    write_fixture(&fixture, "[]");
    let path = fixture.path().join("verification/actor-provenance.json");
    let contents = fs::read_to_string(&path).expect("fixture provenance must be readable");
    fixture.write(
        "verification/actor-provenance.json",
        &contents.replace("\"task\": \"/root\"", "\"task\": \"/rootbad\""),
    );
    let mut diagnostics = Vec::new();
    validate_fixture(&fixture, &mut diagnostics).expect("altered actor registry must parse");
    assert!(
        diagnostics
            .iter()
            .any(|item| item.message().contains("malformed or mismatched provenance"))
    );
}

#[test]
fn unknown_actor_provenance_fields_fail_closed_during_parsing() {
    let fixture = Fixture::new();
    write_fixture(&fixture, "[]");
    let path = fixture.path().join("verification/actor-provenance.json");
    let contents = fs::read_to_string(&path).expect("fixture provenance must be readable");
    fixture.write(
        "verification/actor-provenance.json",
        &contents.replacen(
            "\"actor_id\": \"ACTOR-0001\",",
            "\"actor_id\": \"ACTOR-0001\",\n      \"unknown\": true,",
            1,
        ),
    );
    let mut diagnostics = Vec::new();
    validate_fixture(&fixture, &mut diagnostics)
        .expect("schema failures must aggregate as trust diagnostics");
    assert!(diagnostics.iter().any(|item| item.message().contains("JSON schema")));
}

#[test]
fn unknown_actor_registry_fields_fail_closed_during_parsing() {
    let fixture = Fixture::new();
    write_fixture(&fixture, "[]");
    let path = fixture.path().join("verification/actors.toml");
    let contents = fs::read_to_string(&path).expect("fixture actor registry must be readable");
    fixture.write("verification/actors.toml", &format!("{contents}\nextra = true\n"));
    let mut diagnostics = Vec::new();
    validate_fixture(&fixture, &mut diagnostics)
        .expect("schema failures must aggregate as trust diagnostics");
    assert!(diagnostics.iter().any(|item| item.message().contains("TOML schema")));
}

#[test]
fn missing_and_directory_actor_policies_aggregate_trust_diagnostics() {
    for relative in ["verification/actors.toml", "verification/actor-provenance.json"] {
        for directory in [false, true] {
            let fixture = Fixture::new();
            write_fixture(&fixture, "[]");
            let path = fixture.path().join(relative);
            fs::remove_file(&path).expect("fixture actor policy must be removed");
            if directory {
                fs::create_dir(&path).expect("directory-valued actor policy must be created");
            }
            let mut diagnostics = Vec::new();
            validate_fixture(&fixture, &mut diagnostics)
                .expect("actor policy failures must aggregate as trust diagnostics");
            assert!(
                diagnostics.iter().any(|item| {
                    item.path() == Some(Path::new(relative))
                        && item.message().contains("missing, non-regular")
                }),
                "missing actionable diagnostic for `{relative}` directory={directory}: {diagnostics:?}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn actor_policies_reject_readable_and_broken_symlinks() {
    use std::os::unix::fs::symlink;

    for relative in ["verification/actors.toml", "verification/actor-provenance.json"] {
        for broken in [false, true] {
            let fixture = Fixture::new();
            write_fixture(&fixture, "[]");
            let path = fixture.path().join(relative);
            let target = fixture.path().join(
                if Path::new(relative)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
                {
                    "actor-target.toml"
                } else {
                    "actor-target.json"
                },
            );
            if !broken {
                fs::copy(&path, &target).expect("readable symlink target must be copied");
            }
            fs::remove_file(&path).expect("fixture actor policy must be removed");
            symlink(&target, &path).expect("actor policy symlink must be created");
            let mut diagnostics = Vec::new();
            validate_fixture(&fixture, &mut diagnostics)
                .expect("actor symlink failures must aggregate as trust diagnostics");
            assert!(
                diagnostics.iter().any(|item| {
                    item.path() == Some(Path::new(relative))
                        && item.message().contains("through a symlink")
                }),
                "missing symlink diagnostic for `{relative}` broken={broken}: {diagnostics:?}"
            );
        }
    }
}

#[test]
fn shared_input_review_requires_class_correct_evidence_for_every_affected_package() {
    let fixture = Fixture::new();
    write_fixture(&fixture, "[]");
    let path = fixture.path().join("verification/proof-impact.toml");
    let contents = fs::read_to_string(&path).expect("fixture manifest must be readable");
    let omitted = r#"[[changes.evidence]]
kind = "verus-verify"
owning_crate = "peritus-types"
command = "cargo verus verify --package peritus-types --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20"
"#;
    fixture.write("verification/proof-impact.toml", &contents.replace(omitted, ""));
    let mut diagnostics = Vec::new();
    validate_fixture(&fixture, &mut diagnostics).expect("altered evidence fixture must parse");
    assert!(diagnostics.iter().any(|item| {
        item.message().contains("VerusVerify") && item.message().contains("peritus-types")
    }));
}

#[test]
fn nested_snapshot_fields_fail_closed_during_parsing() {
    let fixture = Fixture::new();
    write_fixture(&fixture, "[]");
    let path = fixture.path().join("verification/proof-impact.toml");
    let contents = fs::read_to_string(&path).expect("fixture manifest must be readable");
    fixture.write(
        "verification/proof-impact.toml",
        &contents.replacen("current = {", "current = { unknown = true,", 1),
    );
    let mut diagnostics = Vec::new();
    validate_fixture(&fixture, &mut diagnostics)
        .expect("schema failures must aggregate as trust diagnostics");
    assert!(diagnostics.iter().any(|item| item.message().contains("TOML schema")));
}
