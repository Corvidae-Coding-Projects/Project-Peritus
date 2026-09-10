use super::*;
use crate::ConversationId;
use peritus_types::WorkspaceId;

fn request() -> InitDiscoveryRequest {
    InitDiscoveryRequest::new(
        WorkbenchQuery::new(
            ConversationId::new([1; 16]).expect("conversation"),
            WorkspaceId::new([2; 16]).expect("workspace"),
        ),
        7,
    )
    .expect("request")
}

fn command(kind: InitCommandKind) -> InitCommand {
    InitCommand::new(
        kind,
        "Cargo.toml".to_owned(),
        "cargo".to_owned(),
        vec![kind.as_str().to_ascii_lowercase()],
        InitCommandVerification::Unverified,
    )
    .expect("command")
}

#[test]
fn exact_managed_section_preserves_all_existing_instruction_bytes() {
    let original = "# Existing instructions\r\n\r\nKeep this text byte-for-byte.".to_owned();
    let digest = peritus_codec::sha256(original.as_bytes());
    let sources = vec![
        InitSourceObservation::new(
            "AGENTS.md".to_owned(),
            InitSourceKind::Instructions,
            digest,
            original.len() as u64,
        )
        .expect("instructions"),
        InitSourceObservation::new(
            "Cargo.toml".to_owned(),
            InitSourceKind::Manifest,
            peritus_codec::sha256(b"[package]"),
            9,
        )
        .expect("manifest"),
    ];
    let proposal = InitProposal::from_discovery(
        request(),
        Sha256Digest::new([3; 32]),
        sources,
        Some((original.clone(), InitFileMode::Regular)),
        vec![command(InitCommandKind::Build)],
    )
    .expect("proposal");

    assert!(proposal.patch().proposed_content().starts_with(&original));
    assert!(proposal.patch().proposed_content().contains("Build (unverified"));
    assert_eq!(proposal.patch().precondition_digest(), Some(digest));
    assert!(proposal.patch().diff().contains("# Existing instructions\r\n"));
    assert!(proposal.patch().diff().starts_with("--- a/AGENTS.md\n+++ b/AGENTS.md\n"));
}

#[test]
fn a_preexisting_managed_section_is_replaced_without_changing_surrounding_bytes() {
    let original = format!(
        "prefix without normalization\r\n\r\n{INIT_SECTION_START}\nold managed text\n\
         {INIT_SECTION_END}\r\ntrailing user text"
    );
    let sources = vec![
        InitSourceObservation::new(
            "AGENTS.md".to_owned(),
            InitSourceKind::Instructions,
            peritus_codec::sha256(original.as_bytes()),
            original.len() as u64,
        )
        .expect("instructions"),
        InitSourceObservation::new(
            "Cargo.toml".to_owned(),
            InitSourceKind::Manifest,
            peritus_codec::sha256(b"[package]"),
            9,
        )
        .expect("manifest"),
    ];
    let proposal = InitProposal::from_discovery(
        request(),
        Sha256Digest::new([3; 32]),
        sources,
        Some((original, InitFileMode::Regular)),
        vec![command(InitCommandKind::Test)],
    )
    .expect("proposal");

    let content = proposal.patch().proposed_content();
    assert!(content.starts_with("prefix without normalization\r\n\r\n"));
    assert!(content.ends_with("\r\ntrailing user text"));
    assert!(!content.contains("old managed text"));
    assert_eq!(content.matches(INIT_SECTION_START).count(), 1);
    assert_eq!(content.matches(INIT_SECTION_END).count(), 1);
}

#[test]
fn proposal_rejects_noncanonical_commands_and_inconsistent_diffs() {
    assert!(
        InitCommand::new(
            InitCommandKind::Build,
            "Cargo.toml".to_owned(),
            "cargo`hidden".to_owned(),
            vec!["build".to_owned()],
            InitCommandVerification::Unverified,
        )
        .is_err()
    );

    let proposed = managed_content(None, &[]).expect("content");
    assert!(
        InitInstructionPatch::new(
            INIT_INSTRUCTION_PATH.to_owned(),
            None,
            None,
            0,
            InitFileMode::Regular,
            proposed,
            "not the exact diff".to_owned(),
        )
        .is_err()
    );
}
