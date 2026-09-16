use super::*;

#[test]
fn failure_bundle_is_bounded_and_records_every_destructive_change() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("source.json"), b"{}\n").expect("source manifest");
    fs::write(fixture.0.join("completion.json"), b"{}\n").expect("completion manifest");
    fs::create_dir(fixture.0.join("scratch")).expect("compiler scratch");
    fs::write(fixture.0.join("scratch/object.o"), vec![b'x'; 20_000]).expect("compiler artifact");
    fs::create_dir(fixture.0.join("mutants.out")).expect("mutation output");
    for path in [
        "filesystem-census.json",
        "selection.json",
        "inventory.stdout",
        "selected-inventory.stdout",
        "context-canary.patch",
        "mutants.out/outcomes.json",
        "timeout-reproducer",
    ] {
        fs::write(fixture.0.join(path), b"evidence\n").expect("essential evidence");
    }
    let diagnostic: Vec<u8> = [vec![b'p'; 10_000], vec![b't'; 10_000]].concat();
    fs::write(fixture.0.join("campaign.stdout"), &diagnostic).expect("diagnostic");
    fs::write(fixture.0.join("disposable.bin"), vec![b'd'; 40_000]).expect("disposable evidence");

    let report =
        bundle::finalize_with_limits(&fixture.0, 32_768, 4_096, 2_048).expect("bounded bundle");

    assert!(report["original_bytes"].as_u64().expect("original bytes") > 60_000);
    assert_eq!(report["compiler_scratch_removed"], true);
    assert_eq!(report["truncated"], true);
    assert!(!fixture.0.join("scratch").exists());
    assert!(!fixture.0.join("disposable.bin").exists());
    for path in [
        "filesystem-census.json",
        "selection.json",
        "inventory.stdout",
        "selected-inventory.stdout",
        "context-canary.patch",
        "mutants.out/outcomes.json",
        "timeout-reproducer",
    ] {
        assert!(fixture.0.join(path).is_file(), "missing essential {path}");
    }
    let retained = fs::read(fixture.0.join("campaign.stdout")).expect("bounded diagnostic");
    assert_eq!(retained.len(), 2_048);
    assert!(retained[..1_024].iter().all(|byte| *byte == b'p'));
    assert!(retained[1_024..].iter().all(|byte| *byte == b't'));
    let changes = report["changes"].as_array().expect("change manifest");
    assert!(changes.iter().any(|change| {
        change["path"] == "campaign.stdout"
            && change["retained_bytes"] == 2_048
            && change["retention"] == "prefix_and_tail"
    }));
    assert!(changes.iter().any(|change| {
        change["path"] == "disposable.bin"
            && change["retained_bytes"] == 0
            && change["retention"] == "removed_after_manifest"
    }));
    let final_bytes: u64 = fs::read_dir(&fixture.0)
        .expect("final bundle")
        .map(|entry| entry.expect("bundle entry").metadata().expect("entry metadata").len())
        .sum();
    assert!(final_bytes <= 32_768, "retained {final_bytes} bytes");
}

#[test]
fn failure_bundle_rejects_required_evidence_larger_than_the_cap() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("source.json"), vec![b's'; 30_000]).expect("required source");
    let error = bundle::finalize_with_limits(&fixture.0, 32_768, 4_096, 2_048)
        .expect_err("required evidence cannot be silently removed");
    assert!(error.render().contains("required discovery manifests use"));
    assert!(fixture.0.join("source.json").is_file());
}

#[cfg(unix)]
#[test]
fn failure_bundle_rejects_symlinks() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    fs::write(fixture.0.join("source.json"), b"{}\n").expect("source manifest");
    symlink(fixture.0.join("source.json"), fixture.0.join("alias.json")).expect("evidence symlink");
    let error = bundle::finalize_with_limits(&fixture.0, 32_768, 4_096, 2_048)
        .expect_err("symlink cannot enter a discovery bundle");
    assert!(error.render().contains("contains a symlink"));
}

#[test]
fn filesystem_census_accepts_stability_and_rejects_source_changes() {
    let fixture = Fixture::new();
    let git = |arguments: &[&str]| {
        let status = Command::new("git")
            .args(arguments)
            .current_dir(&fixture.0)
            .status()
            .expect("run git fixture command");
        assert!(status.success(), "git {arguments:?}");
    };
    git(&["init", "--quiet"]);
    fs::write(fixture.0.join(".gitignore"), "/target/\n").expect("ignore evidence");
    fs::write(fixture.0.join("state.txt"), "initial\n").expect("tracked state");
    git(&["add", ".gitignore", "state.txt"]);
    git(&[
        "-c",
        "user.name=Discovery Test",
        "-c",
        "user.email=discovery@example.invalid",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "--quiet",
        "-m",
        "fixture",
    ]);
    let evidence = fixture.0.join("target/discovery/census");
    fs::create_dir_all(&evidence).expect("evidence directory");
    let initial_revision =
        super::super::capture(&fixture.0, "git", &["rev-parse", "HEAD"]).expect("initial revision");
    super::super::write_json(
        &evidence.join("source.json"),
        &json!({"source_sha": initial_revision.trim()}),
    )
    .expect("write initial source identity");
    fs::write(
        evidence.join("working-tree.patch"),
        super::super::evidence::source_changes(&fixture.0).expect("initial patch"),
    )
    .expect("write initial patch");
    super::super::write_json(
        &evidence.join("untracked-source.json"),
        &json!(
            super::super::evidence::untracked_hashes(&fixture.0).expect("initial untracked files")
        ),
    )
    .expect("write initial untracked census");

    super::super::evidence::verify_workspace_unchanged(&fixture.0, &evidence)
        .expect("unchanged workspace");
    fs::write(fixture.0.join("state.txt"), "changed\n").expect("change tracked state");
    let error = super::super::evidence::verify_workspace_unchanged(&fixture.0, &evidence)
        .expect_err("tracked source change must fail the census");
    assert!(error.render().contains("changed source revision, tracked files"));
    let report: serde_json::Value = serde_json::from_slice(
        &fs::read(evidence.join("filesystem-census.json")).expect("filesystem census"),
    )
    .expect("valid census JSON");
    assert_eq!(report["status"], "changed");
    assert_ne!(report["initial_patch_sha256_base64"], report["final_patch_sha256_base64"]);

    fs::write(fixture.0.join("state.txt"), "committed change\n").expect("committed change");
    git(&["add", "state.txt"]);
    git(&[
        "-c",
        "user.name=Discovery Test",
        "-c",
        "user.email=discovery@example.invalid",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "--quiet",
        "-m",
        "campaign-side commit",
    ]);
    let error = super::super::evidence::verify_workspace_unchanged(&fixture.0, &evidence)
        .expect_err("a clean diff at a changed HEAD must fail the census");
    assert!(error.render().contains("changed source revision"));
    let report: serde_json::Value = serde_json::from_slice(
        &fs::read(evidence.join("filesystem-census.json")).expect("committed census"),
    )
    .expect("valid committed census JSON");
    assert_ne!(report["initial_source_sha"], report["final_source_sha"]);
    assert_eq!(report["initial_patch_sha256_base64"], report["final_patch_sha256_base64"]);
}
