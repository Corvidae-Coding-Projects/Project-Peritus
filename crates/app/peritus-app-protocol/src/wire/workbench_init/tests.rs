use super::*;

fn proposal() -> InitProposal {
    proposal_with_folder_digest(4)
}

fn proposal_with_folder_digest(folder_digest: u8) -> InitProposal {
    let query = crate::WorkbenchQuery::new(
        crate::ConversationId::new([1; 16]).expect("conversation"),
        peritus_types::WorkspaceId::new([2; 16]).expect("workspace"),
    );
    let request = InitDiscoveryRequest::new(query, 3).expect("request");
    let original = "# Existing\n".to_owned();
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
    let command = InitCommand::new(
        InitCommandKind::Build,
        "Cargo.toml".to_owned(),
        "cargo".to_owned(),
        vec!["build".to_owned()],
        InitCommandVerification::Unverified,
    )
    .expect("command");
    InitProposal::from_discovery(
        request,
        peritus_types::Sha256Digest::new([folder_digest; 32]),
        sources,
        Some((original, InitFileMode::Regular)),
        vec![command],
    )
    .expect("proposal")
}

#[test]
fn proposal_fingerprint_changes_with_reviewed_identity() {
    assert_ne!(
        proposal_with_folder_digest(4).fingerprint().expect("original fingerprint"),
        proposal_with_folder_digest(5).fingerprint().expect("changed fingerprint"),
    );
}

#[test]
fn proposal_roundtrip_retains_exact_original_patch_and_structured_argv() {
    let expected = proposal();
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    write_proposal(&mut writer, &expected).expect("encode");
    let bytes = writer.into_bytes();
    let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
    let actual = read_proposal(&mut reader).expect("decode");
    reader.finish().expect("complete value");

    assert_eq!(actual, expected);
    assert_eq!(actual.commands()[0].executable(), "cargo");
    assert_eq!(actual.commands()[0].arguments(), &["build"]);
    assert_eq!(actual.commands()[0].verification(), InitCommandVerification::Unverified);
    assert_eq!(actual.fingerprint().expect("digest"), expected.fingerprint().expect("digest"));

    let canonical = expected.canonical_bytes().expect("canonical bytes");
    let mut separated = CanonicalWriter::new(CodecLimits::PRODUCTION);
    separated.write_fixed(b"peritus-workbench-init-proposal-v1").expect("domain separator");
    separated.write_fixed(&canonical).expect("proposal bytes");
    assert_eq!(
        expected.fingerprint().expect("fingerprint"),
        peritus_codec::sha256(separated.as_slice())
    );
    assert_ne!(expected.fingerprint().expect("fingerprint"), peritus_codec::sha256(&canonical));
}

#[test]
fn decoder_rejects_excessive_source_count_before_reading_sources() {
    let expected = proposal();
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    super::super::workbench::write_query(&mut writer, expected.query()).expect("query");
    writer.write_u64(expected.revision()).expect("revision");
    write_digest(&mut writer, expected.folder_digest()).expect("folder");
    writer.write_collection_len(MAX_INIT_SOURCES + 1).expect("count");
    let bytes = writer.into_bytes();
    let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);

    assert_eq!(
        read_proposal(&mut reader).expect_err("excess count").kind(),
        CodecErrorKind::LimitExceeded
    );
}
