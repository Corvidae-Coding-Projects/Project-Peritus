use super::*;

fn request() -> InitDiscoveryRequest {
    InitDiscoveryRequest::new(
        peritus_app_protocol::WorkbenchQuery::new(
            peritus_app_protocol::ConversationId::new([0x31; 16]).expect("conversation"),
            WorkspaceId::new([0x32; 16]).expect("workspace"),
        ),
        5,
    )
    .expect("request")
}

fn write_fixture(root: &Path) -> Vec<u8> {
    let original = b"# Existing instructions\r\n\r\nNever replace this user guidance.".to_vec();
    fs::write(root.join("AGENTS.md"), &original).expect("instructions");
    fs::write(root.join("Cargo.toml"), "[package]\nname = \"sample\"\nversion = \"0.1.0\"\n")
        .expect("manifest");
    fs::write(
        root.join("package.json"),
        r#"{"scripts":{"build":"touch provider-or-script-invoked","test":"touch provider-or-script-invoked"}}"#,
    )
    .expect("script manifest");
    original
}

#[test]
fn existing_instructions_are_discovered_then_applied_exactly_without_running_scripts() {
    let root = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    let original = write_fixture(root.path());
    let sentinel = root.path().join("provider-or-script-invoked");

    let proposal = discover_init(root.path(), request()).expect("discover");
    assert_eq!(fs::read(root.path().join("AGENTS.md")).expect("unchanged"), original);
    assert!(!sentinel.exists(), "discovery must not run package scripts or providers");
    assert!(
        proposal
            .commands()
            .iter()
            .all(|command| { command.verification() == InitCommandVerification::Unverified })
    );
    assert!(proposal.commands().iter().any(|command| command.kind() == InitCommandKind::Build));
    assert!(proposal.commands().iter().any(|command| command.kind() == InitCommandKind::Test));
    assert!(
        proposal.commands().iter().any(|command| command.kind() == InitCommandKind::Lint),
        "missing lint command in {:?}",
        proposal.commands()
    );
    assert!(
        proposal.commands().iter().any(|command| command.kind() == InitCommandKind::Launch),
        "missing launch command in {:?}",
        proposal.commands()
    );

    let patch = prepare_init_patch(
        root.path(),
        request().query().workspace(),
        Generation::first(),
        RevisionNumber::first(),
        &proposal,
    )
    .expect("prepare exact patch");
    assert_eq!(fs::read(root.path().join("AGENTS.md")).expect("still unchanged"), original);
    assert!(!sentinel.exists(), "patch preparation must not execute discovered commands");

    let plan = patch
        .plan(request().query().workspace(), Generation::first(), RevisionNumber::first())
        .expect("current lower-layer binding");
    peritus_patch::apply_patch(root.path(), transactions.path(), &plan).expect("apply exact patch");
    let applied = fs::read(root.path().join("AGENTS.md")).expect("applied instructions");
    assert_eq!(applied, proposal.patch().proposed_content().as_bytes());
    assert!(applied.starts_with(&original), "all original bytes must remain an exact prefix");
    assert!(!sentinel.exists(), "accepting the config must not execute inventoried commands");
}

#[test]
fn decline_and_prepare_are_no_effects_and_an_intervening_edit_rejects_without_overwrite() {
    let root = tempfile::tempdir().expect("workspace");
    let original = write_fixture(root.path());
    let proposal = discover_init(root.path(), request()).expect("discover");

    let declined = proposal.clone();
    drop(declined);
    assert_eq!(fs::read(root.path().join("AGENTS.md")).expect("declined"), original);

    fs::write(root.path().join("AGENTS.md"), b"independent user edit\n").expect("user edit");
    let error = prepare_init_patch(
        root.path(),
        request().query().workspace(),
        Generation::first(),
        RevisionNumber::first(),
        &proposal,
    )
    .expect_err("stale proposal");
    assert_eq!(error.code(), AppErrorCode::StaleRevision);
    assert_eq!(
        fs::read(root.path().join("AGENTS.md")).expect("preserved user edit"),
        b"independent user edit\n"
    );
    assert!(!root.path().join("provider-or-script-invoked").exists());
}
